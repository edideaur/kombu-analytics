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

use crate::ingest::QueryRange;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsQuery {
    #[serde(alias = "start_at")]
    pub start_at: Option<i64>,
    #[serde(alias = "end_at")]
    pub end_at: Option<i64>,
    pub page: Option<i64>,
    #[serde(alias = "page_size")]
    pub page_size: Option<i64>,
    pub search: Option<String>,
}

pub async fn list(
    Path(website_id): Path<Uuid>,
    Query(query): Query<SessionsQuery>,
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

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let res = kombu_query::get_website_sessions(
        &state.pool,
        website_id,
        start_at,
        end_at,
        page,
        page_size,
        query.search.as_deref(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(serde_json::to_value(res).unwrap_or(json!({}))))
}

pub async fn stats(
    Path(website_id): Path<Uuid>,
    Query(query): Query<QueryRange>,
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

    let res = kombu_query::get_website_session_stats(&state.pool, website_id, start_at, end_at)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(serde_json::to_value(res).unwrap_or(json!({}))))
}

pub async fn weekly(
    Path(website_id): Path<Uuid>,
    Query(query): Query<QueryRange>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(7));

    let matrix = kombu_query::get_weekly_traffic(&state.pool, website_id, start_at, end_at)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!(matrix)))
}

pub async fn get(
    Path((website_id, session_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let res = kombu_query::get_website_session(&state.pool, website_id, session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    match res {
        Some(mut s) => {
            let mut fallback = serde_json::Map::new();
            let o = s.as_object_mut().unwrap_or(&mut fallback);
            o.insert("canDelete".into(), json!(true));
            o.insert("stitchedSessionCount".into(), json!(1));
            Ok(Json(s))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Session not found" })),
        )),
    }
}

pub async fn activity(
    Path((website_id, session_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<SessionsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(30));

    let rows =
        kombu_query::get_session_activity(&state.pool, website_id, session_id, start_at, end_at)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
            })?;

    Ok(Json(json!(rows)))
}

pub async fn properties(
    Path((website_id, session_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT data_key as key, string_value as value, created_at as "createdAt"
            FROM session_data
            WHERE website_id = $1 AND session_id = $2
            ORDER BY data_key ASC
        ) t
        "#,
    )
    .bind(website_id)
    .bind(session_id)
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
    async fn test_sessions_endpoints_full() {
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

        assert!(
            list(
                Path(website_id),
                Query(SessionsQuery {
                    start_at: None,
                    end_at: None,
                    page: Some(1),
                    page_size: Some(10),
                    search: Some("chrome".into()),
                }),
                State(state.clone())
            )
            .await
            .is_ok()
        );

        assert!(
            stats(
                Path(website_id),
                Query(QueryRange::default()),
                State(state.clone())
            )
            .await
            .is_ok()
        );

        assert!(
            weekly(
                Path(website_id),
                Query(QueryRange::default()),
                State(state.clone())
            )
            .await
            .is_ok()
        );

        assert!(
            get(Path((website_id, session_id)), State(state.clone()))
                .await
                .is_err()
        );

        let _ =
            sqlx::query(r#"INSERT INTO "website" (website_id, name, domain) VALUES ($1, $2, $3)"#)
                .bind(website_id)
                .bind("Sessions Web")
                .bind(format!("sess-{website_id}.com"))
                .execute(&pool)
                .await;

        let _ = sqlx::query(
            r#"INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, distinct_id)
               VALUES ($1, $2, 'Chrome', 'Linux', 'desktop', '1920x1080', 'en', 'US', 'dist1')"#,
        )
        .bind(session_id)
        .bind(website_id)
        .execute(&pool)
        .await;

        let event_id = Uuid::now_v7();
        let _ = sqlx::query(
            r#"INSERT INTO "website_event" (event_id, website_id, session_id, event_name, url_path)
               VALUES ($1, $2, $3, 'test_event', '/test')"#,
        )
        .bind(event_id)
        .bind(website_id)
        .bind(session_id)
        .execute(&pool)
        .await;

        assert!(
            get(Path((website_id, session_id)), State(state.clone()))
                .await
                .is_ok()
        );

        assert!(
            activity(
                Path((website_id, session_id)),
                Query(SessionsQuery {
                    start_at: None,
                    end_at: None,
                    page: None,
                    page_size: None,
                    search: None,
                }),
                State(state.clone())
            )
            .await
            .is_ok()
        );

        assert!(
            properties(Path((website_id, session_id)), State(state.clone()))
                .await
                .is_ok()
        );

        let _ = sqlx::query(r#"DELETE FROM "session" WHERE session_id = $1"#)
            .bind(session_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;

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

        let dummy_sq = Query(SessionsQuery {
            start_at: None,
            end_at: None,
            page: None,
            page_size: None,
            search: None,
        });
        assert!(
            list(Path(website_id), dummy_sq, State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            stats(
                Path(website_id),
                Query(QueryRange::default()),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            weekly(
                Path(website_id),
                Query(QueryRange::default()),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            get(Path((website_id, session_id)), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            activity(
                Path((website_id, session_id)),
                Query(SessionsQuery {
                    start_at: None,
                    end_at: None,
                    page: None,
                    page_size: None,
                    search: None
                }),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            properties(Path((website_id, session_id)), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
