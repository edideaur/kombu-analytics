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

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventsQuery {
    #[serde(alias = "start_at")]
    pub start_at: Option<i64>,
    #[serde(alias = "end_at")]
    pub end_at: Option<i64>,
    pub page: Option<i64>,
    #[serde(alias = "page_size")]
    pub page_size: Option<i64>,
    pub search: Option<String>,
    pub unit: Option<String>,
    pub compare: Option<String>,
}

pub async fn list(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventsQuery>,
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

    let res = kombu_query::get_website_events(
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

pub async fn series(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventsQuery>,
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

    let unit = query.unit.as_deref().unwrap_or("day");

    let rows = kombu_query::get_pageview_stats(&state.pool, website_id, start_at, end_at, unit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(serde_json::to_value(rows).unwrap_or(json!([]))))
}

pub async fn stats(
    Path(website_id): Path<Uuid>,
    Query(query): Query<EventsQuery>,
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

    let duration = end_at - start_at;
    let prev_end = start_at;
    let prev_start = prev_end - duration;

    let data = kombu_query::get_website_event_stats(&state.pool, website_id, start_at, end_at)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    let comparison =
        kombu_query::get_website_event_stats(&state.pool, website_id, prev_start, prev_end)
            .await
            .unwrap_or(kombu_query::WebsiteEventStats {
                events: 0,
                visitors: 0,
                visits: 0,
                unique_events: 0,
            });

    let res = json!({
        "data": {
            "events": data.events,
            "visitors": data.visitors,
            "visits": data.visits,
            "uniqueEvents": data.unique_events,
            "comparison": comparison
        }
    });

    Ok(Json(res))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_events_endpoints_full() {
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
        let q = Query(EventsQuery {
            start_at: None,
            end_at: None,
            page: Some(1),
            page_size: Some(10),
            search: Some("test".into()),
            unit: Some("day".into()),
            compare: None,
        });

        assert!(
            list(Path(website_id), q.clone(), State(state.clone()))
                .await
                .is_ok()
        );

        assert!(
            series(Path(website_id), q.clone(), State(state.clone()))
                .await
                .is_ok()
        );

        assert!(
            stats(Path(website_id), q.clone(), State(state.clone()))
                .await
                .is_ok()
        );

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

        assert!(
            list(Path(website_id), q.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            series(Path(website_id), q.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            stats(Path(website_id), q.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
