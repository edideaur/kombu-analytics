#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

const DEFAULT_COLOR: &str = "primary";
const ERR_ANNOTATION_NOT_FOUND: &str = "Annotation not found";
const ERR_FAILED_FETCH: &str = "Failed to fetch annotations";
const ERR_FAILED_CREATE: &str = "Failed to create annotation";
const ERR_FAILED_UPDATE: &str = "Failed to update annotation";
const ERR_FAILED_DELETE: &str = "Failed to delete annotation";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationInput {
    pub website_id: Option<Uuid>,
    pub date: Option<DateTime<Utc>>,
    pub title: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationUpdateInput {
    pub date: Option<DateTime<Utc>>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationQuery {
    pub start_at: Option<i64>,
    pub end_at: Option<i64>,
}

pub async fn list_for_website(
    Path(website_id): Path<Uuid>,
    Query(query): Query<AnnotationQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let start_date = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or(DateTime::UNIX_EPOCH);

    let end_date = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let rows = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                annotation_id as id,
                website_id as "websiteId",
                user_id as "userId",
                date,
                title,
                description,
                color,
                created_at as "createdAt",
                updated_at as "updatedAt"
            FROM "website_annotation"
            WHERE website_id = $1
              AND date BETWEEN $2 AND $3
            ORDER BY date ASC
        ) t
        "#,
    )
    .bind(website_id)
    .bind(start_date)
    .bind(end_date)
    .fetch_one(&state.pool)
    .await
    .unwrap_or_else(|_| json!([]));

    let count = rows.as_array().map_or(0, std::vec::Vec::len);

    Ok(Json(json!({
        "data": rows,
        "count": count
    })))
}

pub async fn create(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(input): Json<AnnotationInput>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let annotation_id = Uuid::now_v7();
    let date = input.date.unwrap_or_else(Utc::now);
    let color = input.color.as_deref().unwrap_or(DEFAULT_COLOR);

    sqlx::query(
        r#"
        INSERT INTO "website_annotation" (
            annotation_id, website_id, date, title, description, color
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(annotation_id)
    .bind(website_id)
    .bind(date)
    .bind(&input.title)
    .bind(&input.description)
    .bind(color)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": ERR_FAILED_CREATE, "details": e.to_string() })),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": annotation_id,
            "websiteId": website_id,
            "date": date,
            "title": input.title,
            "description": input.description,
            "color": color
        })),
    ))
}

pub async fn get(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                annotation_id as id,
                website_id as "websiteId",
                user_id as "userId",
                date,
                title,
                description,
                color,
                created_at as "createdAt",
                updated_at as "updatedAt"
            FROM "website_annotation"
            WHERE annotation_id = $1
        ) t
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(val) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": ERR_ANNOTATION_NOT_FOUND })),
        ));
    };

    Ok(Json(val))
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(input): Json<AnnotationUpdateInput>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let existing = sqlx::query_as::<_, (DateTime<Utc>, String, Option<String>, String)>(
        r#"
        SELECT date, title, description, color
        FROM "website_annotation"
        WHERE annotation_id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": ERR_FAILED_FETCH, "details": e.to_string() })),
        )
    })?;

    let Some((old_date, old_title, old_desc, old_color)) = existing else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": ERR_ANNOTATION_NOT_FOUND })),
        ));
    };

    let new_date = input.date.unwrap_or(old_date);
    let new_title = input.title.as_deref().unwrap_or(&old_title);
    let new_desc = input.description.as_ref().or(old_desc.as_ref());
    let new_color = input.color.as_deref().unwrap_or(&old_color);
    let now = Utc::now();

    sqlx::query(
        r#"
        UPDATE "website_annotation"
        SET date = $2,
            title = $3,
            description = $4,
            color = $5,
            updated_at = $6
        WHERE annotation_id = $1
        "#,
    )
    .bind(id)
    .bind(new_date)
    .bind(new_title)
    .bind(new_desc)
    .bind(new_color)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": ERR_FAILED_UPDATE, "details": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "id": id,
        "date": new_date,
        "title": new_title,
        "description": new_desc,
        "color": new_color,
        "updatedAt": now
    })))
}

pub async fn delete(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let result = sqlx::query(
        r#"
        DELETE FROM "website_annotation"
        WHERE annotation_id = $1
        "#,
    )
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": ERR_FAILED_DELETE, "details": e.to_string() })),
        )
    })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": ERR_ANNOTATION_NOT_FOUND })),
        ));
    }

    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_annotations_endpoints_full() {
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
                .bind("Annotations Web")
                .bind(format!("ann-{website_id}.com"))
                .execute(&pool)
                .await;

        let input = AnnotationInput {
            website_id: Some(website_id),
            date: None,
            title: "Product Launch".into(),
            description: Some("Launch v1".into()),
            color: None,
        };
        let res_create = create(Path(website_id), State(state.clone()), Json(input)).await;
        assert!(res_create.is_ok());
        let ann_id: Uuid = res_create.unwrap().1.0["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        let q = Query(AnnotationQuery {
            start_at: None,
            end_at: None,
        });
        assert!(
            list_for_website(Path(website_id), q, State(state.clone()))
                .await
                .is_ok()
        );

        assert!(get(Path(ann_id), State(state.clone())).await.is_ok());
        assert!(
            get(Path(Uuid::now_v7()), State(state.clone()))
                .await
                .is_err()
        );

        let upd_input = AnnotationUpdateInput {
            date: None,
            title: Some("Updated Launch".into()),
            description: None,
            color: Some("secondary".into()),
        };
        assert!(
            update(Path(ann_id), State(state.clone()), Json(upd_input))
                .await
                .is_ok()
        );
        assert!(
            update(
                Path(Uuid::now_v7()),
                State(state.clone()),
                Json(AnnotationUpdateInput {
                    date: None,
                    title: None,
                    description: None,
                    color: None
                })
            )
            .await
            .is_err()
        );

        let upd_toolong = AnnotationUpdateInput {
            date: None,
            title: Some("x".repeat(300)),
            description: None,
            color: None,
        };
        assert!(
            update(Path(ann_id), State(state.clone()), Json(upd_toolong))
                .await
                .is_err()
        );

        assert!(delete(Path(ann_id), State(state.clone())).await.is_ok());
        assert!(
            delete(Path(Uuid::now_v7()), State(state.clone()))
                .await
                .is_err()
        );

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
            create(
                Path(website_id),
                State(err_state.clone()),
                Json(AnnotationInput {
                    website_id: None,
                    date: None,
                    title: "err".into(),
                    description: None,
                    color: None
                })
            )
            .await
            .is_err()
        );
        assert!(
            list_for_website(
                Path(website_id),
                Query(AnnotationQuery {
                    start_at: None,
                    end_at: None
                }),
                State(err_state.clone())
            )
            .await
            .is_ok()
        );
        assert!(
            update(
                Path(ann_id),
                State(err_state.clone()),
                Json(AnnotationUpdateInput {
                    date: None,
                    title: None,
                    description: None,
                    color: None
                })
            )
            .await
            .is_err()
        );
        assert!(
            delete(Path(ann_id), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
