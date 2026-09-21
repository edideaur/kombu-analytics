#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::ingest::QueryRange;
use crate::router::AppState;

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

    let row = sqlx::query_as::<_, (i64, i64, String)>(
        r#"
        SELECT
            COUNT(*)::bigint as "totalOrders",
            COUNT(DISTINCT session_id)::bigint as "uniqueCustomers",
            COALESCE(SUM(revenue), 0)::text as "totalRevenue"
        FROM "revenue"
        WHERE website_id = $1 AND created_at BETWEEN $2 AND $3
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, 0, "0".to_string()));

    Ok(Json(json!({
        "totalOrders": row.0,
        "uniqueCustomers": row.1,
        "totalRevenue": row.2
    })))
}

pub async fn chart(
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

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                TO_CHAR(DATE_TRUNC('day', created_at), 'YYYY-MM-DD 00:00:00') as x,
                COALESCE(SUM(revenue), 0) as y
            FROM "revenue"
            WHERE website_id = $1 AND created_at BETWEEN $2 AND $3
            GROUP BY 1
            ORDER BY 1 ASC
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

pub async fn metrics(
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

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                event_name as x,
                COALESCE(SUM(revenue), 0) as y
            FROM "revenue"
            WHERE website_id = $1 AND created_at BETWEEN $2 AND $3
            GROUP BY 1
            ORDER BY 2 DESC
            LIMIT 10
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

pub async fn sessions(
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

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                r.session_id as id,
                r.event_name as "eventName",
                r.revenue,
                r.currency,
                r.created_at as "createdAt"
            FROM "revenue" r
            WHERE r.website_id = $1 AND r.created_at BETWEEN $2 AND $3
            ORDER BY r.created_at DESC
            LIMIT 50
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
        "pageSize": 50
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_revenue_endpoints_full() {
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
        let q = Query(QueryRange::default());

        assert!(stats(Path(website_id), q.clone(), State(state.clone())).await.is_ok());

        assert!(chart(Path(website_id), q.clone(), State(state.clone())).await.is_ok());

        assert!(metrics(Path(website_id), q.clone(), State(state.clone())).await.is_ok());

        assert!(sessions(Path(website_id), q.clone(), State(state.clone())).await.is_ok());

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

        assert!(chart(Path(website_id), q.clone(), State(err_state.clone())).await.is_err());
        assert!(metrics(Path(website_id), q.clone(), State(err_state.clone())).await.is_err());
        assert!(sessions(Path(website_id), q.clone(), State(err_state.clone())).await.is_err());
    }
}
