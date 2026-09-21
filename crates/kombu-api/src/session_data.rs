#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct SessionDataQuery {
    #[serde(rename = "startAt")]
    pub start_at: Option<i64>,
    #[serde(rename = "endAt")]
    pub end_at: Option<i64>,
    #[serde(rename = "propertyName")]
    pub property_name: Option<String>,
    #[serde(rename = "dataType")]
    pub data_type: Option<i32>,
}

pub async fn properties(
    Path(website_id): Path<Uuid>,
    Query(query): Query<SessionDataQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(1));

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                data_key as "propertyName",
                data_type as "dataType",
                count(*)::bigint as "total"
            FROM session_data
            WHERE website_id = $1
              AND created_at BETWEEN $2 AND $3
              AND ($4::text IS NULL OR data_key = $4)
            GROUP BY data_key, data_type
            ORDER BY total DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(query.property_name)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(rows))
}

pub async fn values(
    Path(website_id): Path<Uuid>,
    Query(query): Query<SessionDataQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(1));

    let property_name = query.property_name.unwrap_or_default();

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                string_value as "value",
                count(*)::bigint as "total"
            FROM session_data
            WHERE website_id = $1
              AND data_key = $2
              AND created_at BETWEEN $3 AND $4
            GROUP BY string_value
            ORDER BY total DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(website_id)
    .bind(property_name)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(rows))
}

pub async fn stats(
    Path(website_id): Path<Uuid>,
    Query(query): Query<SessionDataQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(1));

    let property_name = query.property_name.unwrap_or_default();

    let row = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT
            COUNT(DISTINCT session_id)::bigint as sessions,
            COUNT(*)::bigint as total
        FROM session_data
        WHERE website_id = $1
          AND data_key = $2
          AND created_at BETWEEN $3 AND $4
        "#,
    )
    .bind(website_id)
    .bind(property_name)
    .bind(start_at)
    .bind(end_at)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let (sessions, total) = row.unwrap_or((0, 0));

    Ok(Json(json!({
        "sessions": sessions,
        "total": total
    })))
}

pub async fn pivot(
    Path(website_id): Path<Uuid>,
    Query(query): Query<SessionDataQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(1));

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                sd.data_key as "propertyName",
                sd.data_type as "dataType",
                sd.string_value as "value",
                count(*)::bigint as "total"
            FROM session_data sd
            WHERE sd.website_id = $1
              AND sd.created_at BETWEEN $2 AND $3
            GROUP BY sd.data_key, sd.data_type, sd.string_value
            ORDER BY "total" DESC
            LIMIT 500
        ) t
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(rows))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_data_endpoints_full() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let state = AppState {
            pool: pool.clone(),
            session_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            website_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            rate_limiter: crate::rate_limit::RateLimiter::new(),
            ingest_queue: crate::queue::IngestQueue::new(&pool, 16, 2),
            app_secret: std::sync::Arc::new("kombu-secret".into()),
        };

        let website_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let session_data_id = Uuid::now_v7();
        let now = Utc::now();

        let _ = sqlx::query(
            r#"INSERT INTO "website" (website_id, name, domain) VALUES ($1, $2, $3)"#,
        )
        .bind(website_id)
        .bind("Session Data Test Web")
        .bind(format!("sd-{website_id}.com"))
        .execute(&pool)
        .await;

        let _ = sqlx::query(
            r#"INSERT INTO "session" (session_id, website_id, hostname, browser, os, device, screen, language, country, distinct_id)
               VALUES ($1, $2, 'localhost', 'Chrome', 'Linux', 'desktop', '1920x1080', 'en', 'US', 'dist1')"#,
        )
        .bind(session_id)
        .bind(website_id)
        .execute(&pool)
        .await;

        let _ = sqlx::query(
            r#"INSERT INTO "session_data" (session_data_id, website_id, session_id, data_key, string_value, number_value, data_type, created_at)
               VALUES ($1, $2, $3, 'tier', 'gold', 1.0, 1, $4)"#,
        )
        .bind(session_data_id)
        .bind(website_id)
        .bind(session_id)
        .bind(now)
        .execute(&pool)
        .await;

        assert!(properties(Path(website_id), Query(SessionDataQuery {
            start_at: None,
            end_at: None,
            property_name: Some("tier".into()),
            data_type: None,
        }), State(state.clone())).await.is_ok());

        assert!(properties(Path(website_id), Query(SessionDataQuery {
            start_at: None,
            end_at: None,
            property_name: None,
            data_type: None,
        }), State(state.clone())).await.is_ok());

        assert!(values(Path(website_id), Query(SessionDataQuery {
            start_at: None,
            end_at: None,
            property_name: Some("tier".into()),
            data_type: None,
        }), State(state.clone())).await.is_ok());

        assert!(stats(Path(website_id), Query(SessionDataQuery {
            start_at: None,
            end_at: None,
            property_name: Some("tier".into()),
            data_type: None,
        }), State(state.clone())).await.is_ok());

        assert!(pivot(Path(website_id), Query(SessionDataQuery {
            start_at: None,
            end_at: None,
            property_name: None,
            data_type: None,
        }), State(state.clone())).await.is_ok());

        let _ = sqlx::query(r#"DELETE FROM "session_data" WHERE session_data_id = $1"#).bind(session_data_id).execute(&pool).await;
        let _ = sqlx::query(r#"DELETE FROM "session" WHERE session_id = $1"#).bind(session_id).execute(&pool).await;
        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#).bind(website_id).execute(&pool).await;

        let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        closed_pool.close().await;
        let err_state = AppState {
            pool: closed_pool,
            session_cache: state.session_cache.clone(),
            website_cache: state.website_cache.clone(),
            rate_limiter: state.rate_limiter.clone(),
            ingest_queue: state.ingest_queue.clone(),
            app_secret: state.app_secret.clone(),
        };

        let dummy_q = Query(SessionDataQuery {
            start_at: None,
            end_at: None,
            property_name: None,
            data_type: None,
        });
        assert!(properties(Path(website_id), dummy_q, State(err_state.clone())).await.is_err());
        assert!(values(Path(website_id), Query(SessionDataQuery { start_at: None, end_at: None, property_name: None, data_type: None }), State(err_state.clone())).await.is_err());
        assert!(stats(Path(website_id), Query(SessionDataQuery { start_at: None, end_at: None, property_name: None, data_type: None }), State(err_state.clone())).await.is_err());
        assert!(pivot(Path(website_id), Query(SessionDataQuery { start_at: None, end_at: None, property_name: None, data_type: None }), State(err_state.clone())).await.is_err());
    }
}
