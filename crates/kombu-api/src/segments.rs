#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

pub async fn list(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                segment_id as id,
                website_id as "websiteId",
                type,
                name,
                parameters,
                created_at as "createdAt"
            FROM "segment"
            WHERE website_id = $1
            ORDER BY name ASC
        ) t
        "#,
    )
    .bind(website_id)
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

pub async fn create(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let segment_id = Uuid::now_v7();
    let r#type = body["type"].as_str().unwrap_or("segment");
    let name = body["name"].as_str().unwrap_or("New Segment");
    let parameters = body.get("parameters").cloned().unwrap_or(json!({}));

    sqlx::query(
        r#"
        INSERT INTO "segment" (segment_id, website_id, type, name, parameters, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
        "#,
    )
    .bind(segment_id)
    .bind(website_id)
    .bind(r#type)
    .bind(name)
    .bind(parameters)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "id": segment_id,
        "websiteId": website_id,
        "type": r#type,
        "name": name
    })))
}

pub async fn delete(
    Path((website_id, segment_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(r#"DELETE FROM "segment" WHERE website_id = $1 AND segment_id = $2"#)
        .bind(website_id)
        .bind(segment_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_segments_endpoints_full() {
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
        let _ =
            sqlx::query(r#"INSERT INTO "website" (website_id, name, domain) VALUES ($1, $2, $3)"#)
                .bind(website_id)
                .bind("Segment Test Web")
                .bind(format!("seg-{website_id}.com"))
                .execute(&pool)
                .await;

        let res_create = create(
            Path(website_id),
            State(state.clone()),
            Json(json!({
                "name": "Power Users",
                "type": "custom",
                "parameters": { "country": "US" }
            })),
        )
        .await;
        assert!(res_create.is_ok());
        let seg_id: Uuid = res_create.unwrap().0["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        let res_list = list(Path(website_id), State(state.clone())).await;
        assert!(res_list.is_ok());

        let res_del = delete(Path((website_id, seg_id)), State(state.clone())).await;
        assert!(res_del.is_ok());

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

        assert!(
            list(Path(website_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            create(Path(website_id), State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
        assert!(
            delete(Path((website_id, seg_id)), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
