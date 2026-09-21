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
pub struct EventDataQuery {
    #[serde(rename = "startAt")]
    pub start_at: Option<i64>,
    #[serde(rename = "endAt")]
    pub end_at: Option<i64>,
    pub key: Option<String>,
    #[serde(rename = "propertyName")]
    pub property_name: Option<String>,
    pub unit: Option<String>,
    pub timezone: Option<String>,
}

pub async fn list(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventDataQuery>,
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
                ed.website_event_id as "eventId",
                we.event_name as "eventName",
                jsonb_agg(jsonb_build_object('key', ed.data_key, 'value', ed.string_value)) as "eventProperties"
            FROM event_data ed
            JOIN website_event we ON we.event_id = ed.website_event_id
            WHERE ed.website_id = $1
              AND ed.created_at BETWEEN $2 AND $3
            GROUP BY ed.website_event_id, we.event_name
            LIMIT 100
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

    let count = rows.as_array().map_or(0, |a| a.len());

    Ok(Json(json!({
        "data": rows,
        "count": count,
        "page": 1,
        "pageSize": 100
    })))
}

pub async fn properties(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventDataQuery>,
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
                count(*)::bigint as "total"
            FROM event_data
            WHERE website_id = $1
              AND created_at BETWEEN $2 AND $3
            GROUP BY data_key
            ORDER BY total DESC
            LIMIT 100
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

pub async fn values(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventDataQuery>,
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

    let property_key = query.key.or(query.property_name).unwrap_or_default();

    if let Some(unit_str) = query.unit.as_deref() {
        let valid_unit = match unit_str {
            "hour" | "day" | "week" | "month" | "year" => unit_str,
            _ => "day",
        };
        let sql = format!(
            r#"
            SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
                SELECT
                    to_char(date_trunc('{valid_unit}', created_at), 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as "date",
                    string_value as "value",
                    count(*)::bigint as "total"
                FROM event_data
                WHERE website_id = $1
                  AND data_key = $2
                  AND created_at BETWEEN $3 AND $4
                GROUP BY 1, 2
                ORDER BY 1 ASC, "total" DESC
                LIMIT 500
            ) t
            "#
        );

        let rows = sqlx::query_scalar::<_, serde_json::Value>(&sql)
            .bind(website_id)
            .bind(property_key)
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

        return Ok(Json(rows));
    }

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                string_value as "value",
                count(*)::bigint as "total"
            FROM event_data
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
    .bind(property_key)
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

pub async fn get_by_id(
    Path((website_id, event_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                ed.website_id as "websiteId",
                ed.website_event_id as "eventId",
                we.event_name as "eventName",
                ed.data_key as "dataKey",
                ed.string_value as "stringValue",
                ed.number_value as "numberValue",
                ed.date_value as "dateValue",
                ed.data_type as "dataType",
                ed.created_at as "createdAt"
            FROM event_data ed
            JOIN website_event we ON we.event_id = ed.website_event_id
            WHERE ed.website_id = $1 AND ed.website_event_id = $2
            ORDER BY ed.created_at ASC
        ) t
        "#,
    )
    .bind(website_id)
    .bind(event_id)
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

pub async fn fields(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventDataQuery>,
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
                ed.data_key as "propertyName",
                ed.data_type as "dataType",
                count(*)::bigint as "total"
            FROM event_data ed
            WHERE ed.website_id = $1
              AND ed.created_at BETWEEN $2 AND $3
            GROUP BY ed.data_key, ed.data_type
            ORDER BY "total" DESC, "propertyName" ASC
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

pub async fn events(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventDataQuery>,
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
            SELECT DISTINCT
                we.event_name as "eventName",
                count(*)::bigint as "total"
            FROM website_event we
            JOIN event_data ed ON ed.website_event_id = we.event_id
            WHERE we.website_id = $1
              AND we.created_at BETWEEN $2 AND $3
            GROUP BY we.event_name
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

pub async fn pivot(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventDataQuery>,
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
                ed.data_key as "propertyName",
                ed.data_type as "dataType",
                ed.string_value as "value",
                count(*)::bigint as "total"
            FROM event_data ed
            WHERE ed.website_id = $1
              AND ed.created_at BETWEEN $2 AND $3
            GROUP BY ed.data_key, ed.data_type, ed.string_value
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
    async fn test_event_data_endpoints_full() {
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
        let event_id = Uuid::now_v7();
        let event_data_id = Uuid::now_v7();
        let now = Utc::now();

        let _ =
            sqlx::query(r#"INSERT INTO "website" (website_id, name, domain) VALUES ($1, $2, $3)"#)
                .bind(website_id)
                .bind("Event Data Test Web")
                .bind(format!("ed-{website_id}.com"))
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
            r#"INSERT INTO "website_event" (event_id, website_id, session_id, event_name, url_path)
               VALUES ($1, $2, $3, 'signup', '/register')"#,
        )
        .bind(event_id)
        .bind(website_id)
        .bind(session_id)
        .execute(&pool)
        .await;

        let _ = sqlx::query(
            r#"INSERT INTO "event_data" (event_data_id, website_id, website_event_id, data_key, string_value, number_value, data_type, created_at)
               VALUES ($1, $2, $3, 'plan', 'pro', 29.0, 1, $4)"#,
        )
        .bind(event_data_id)
        .bind(website_id)
        .bind(event_id)
        .bind(now)
        .execute(&pool)
        .await;

        let q = EventDataQuery {
            start_at: Some((now - Duration::hours(1)).timestamp_millis()),
            end_at: Some((now + Duration::hours(1)).timestamp_millis()),
            key: Some("plan".into()),
            property_name: None,
            unit: None,
            timezone: None,
        };

        let res_list = list(
            Path(website_id),
            Query(EventDataQuery {
                start_at: None,
                end_at: None,
                key: None,
                property_name: None,
                unit: None,
                timezone: None,
            }),
            State(state.clone()),
        )
        .await;
        assert!(res_list.is_ok());

        let res_props = properties(Path(website_id), Query(q), State(state.clone())).await;
        assert!(res_props.is_ok());

        let res_vals = values(
            Path(website_id),
            Query(EventDataQuery {
                start_at: None,
                end_at: None,
                key: None,
                property_name: Some("plan".into()),
                unit: None,
                timezone: None,
            }),
            State(state.clone()),
        )
        .await;
        assert!(res_vals.is_ok());

        for u in ["hour", "day", "invalid"] {
            let res_vals_unit = values(
                Path(website_id),
                Query(EventDataQuery {
                    start_at: None,
                    end_at: None,
                    key: Some("plan".into()),
                    property_name: None,
                    unit: Some(u.into()),
                    timezone: None,
                }),
                State(state.clone()),
            )
            .await;
            assert!(res_vals_unit.is_ok());
        }

        let res_get = get_by_id(Path((website_id, event_id)), State(state.clone())).await;
        assert!(res_get.is_ok());

        let res_fields = fields(
            Path(website_id),
            Query(EventDataQuery {
                start_at: None,
                end_at: None,
                key: None,
                property_name: None,
                unit: None,
                timezone: None,
            }),
            State(state.clone()),
        )
        .await;
        assert!(res_fields.is_ok());

        let res_events = events(
            Path(website_id),
            Query(EventDataQuery {
                start_at: None,
                end_at: None,
                key: None,
                property_name: None,
                unit: None,
                timezone: None,
            }),
            State(state.clone()),
        )
        .await;
        assert!(res_events.is_ok());

        let res_pivot = pivot(
            Path(website_id),
            Query(EventDataQuery {
                start_at: None,
                end_at: None,
                key: None,
                property_name: None,
                unit: None,
                timezone: None,
            }),
            State(state.clone()),
        )
        .await;
        assert!(res_pivot.is_ok());

        let _ = sqlx::query(r#"DELETE FROM "event_data" WHERE event_data_id = $1"#)
            .bind(event_data_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "website_event" WHERE event_id = $1"#)
            .bind(event_id)
            .execute(&pool)
            .await;
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

        let dummy_q = Query(EventDataQuery {
            start_at: None,
            end_at: None,
            key: None,
            property_name: None,
            unit: None,
            timezone: None,
        });
        assert!(
            list(Path(website_id), dummy_q, State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            properties(
                Path(website_id),
                Query(EventDataQuery {
                    start_at: None,
                    end_at: None,
                    key: None,
                    property_name: None,
                    unit: None,
                    timezone: None
                }),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            values(
                Path(website_id),
                Query(EventDataQuery {
                    start_at: None,
                    end_at: None,
                    key: None,
                    property_name: None,
                    unit: None,
                    timezone: None
                }),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            values(
                Path(website_id),
                Query(EventDataQuery {
                    start_at: None,
                    end_at: None,
                    key: None,
                    property_name: None,
                    unit: Some("day".into()),
                    timezone: None
                }),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            get_by_id(Path((website_id, event_id)), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            fields(
                Path(website_id),
                Query(EventDataQuery {
                    start_at: None,
                    end_at: None,
                    key: None,
                    property_name: None,
                    unit: None,
                    timezone: None
                }),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            events(
                Path(website_id),
                Query(EventDataQuery {
                    start_at: None,
                    end_at: None,
                    key: None,
                    property_name: None,
                    unit: None,
                    timezone: None
                }),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            pivot(
                Path(website_id),
                Query(EventDataQuery {
                    start_at: None,
                    end_at: None,
                    key: None,
                    property_name: None,
                    unit: None,
                    timezone: None
                }),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
    }
}
