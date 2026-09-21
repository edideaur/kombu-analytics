#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

pub async fn list(State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT board_id as id, name, description, type, parameters, user_id as "userId", team_id as "teamId", created_at as "createdAt"
            FROM "board"
            ORDER BY created_at DESC
        ) t
        "#,
    )
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

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let board_id = Uuid::now_v7();
    let name = body["name"].as_str().unwrap_or("New Board");
    let description = body["description"].as_str().unwrap_or("");
    let r#type = body["type"].as_str().unwrap_or("board");
    let user_id = body["userId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let team_id = body["teamId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let parameters = body.get("parameters").cloned().unwrap_or(json!({}));

    sqlx::query(
        r#"
        INSERT INTO "board" (board_id, user_id, team_id, name, description, type, parameters, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        "#,
    )
    .bind(board_id)
    .bind(user_id)
    .bind(team_id)
    .bind(name)
    .bind(description)
    .bind(r#type)
    .bind(parameters)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(board_id), State(state)).await
}

pub async fn get(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT board_id as id, name, description, type, parameters, user_id as "userId", team_id as "teamId", created_at as "createdAt"
            FROM "board"
            WHERE board_id = $1
        ) t
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some(b) => Ok(Json(b)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Board not found" })),
        )),
    }
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str();
    let description = body["description"].as_str();
    let parameters = body.get("parameters").cloned();

    sqlx::query(
        r#"
        UPDATE "board"
        SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            parameters = COALESCE($4, parameters),
            updated_at = NOW()
        WHERE board_id = $1
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(parameters)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(id), State(state)).await
}

pub async fn delete(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(r#"DELETE FROM "board" WHERE board_id = $1"#)
        .bind(id)
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

pub async fn clone(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let source_board =
        sqlx::query_as::<_, (String, String, String, Option<Uuid>, Option<Uuid>, Value)>(
            r#"
        SELECT name, description, type, user_id, team_id, parameters
        FROM "board"
        WHERE board_id = $1
        "#,
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    let Some((
        source_name,
        source_desc,
        source_type,
        source_user_id,
        source_team_id,
        source_params,
    )) = source_board
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Board not found" })),
        ));
    };

    let new_name = body["name"]
        .as_str()
        .map_or_else(|| format!("{source_name} (Copy)"), ToString::to_string);
    let new_desc = body["description"]
        .as_str()
        .map_or(source_desc, ToString::to_string);
    let new_params = body.get("parameters").cloned().unwrap_or(source_params);

    let new_id = Uuid::now_v7();

    sqlx::query(
        r#"
        INSERT INTO "board" (board_id, user_id, team_id, name, description, type, parameters, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        "#,
    )
    .bind(new_id)
    .bind(source_user_id)
    .bind(source_team_id)
    .bind(&new_name)
    .bind(&new_desc)
    .bind(&source_type)
    .bind(&new_params)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(new_id), State(state)).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_boards_crud_and_clone() {
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

        let _ = create(
            State(state.clone()),
            Json(json!({
                "name": "Invalid UUIDs",
                "userId": "not-a-uuid",
                "teamId": "not-a-uuid"
            })),
        )
        .await;

        let body_create = json!({
            "name": "Test Board",
            "description": "Board for tests",
            "type": "custom",
            "parameters": { "layout": "grid" }
        });
        let res_create = create(State(state.clone()), Json(body_create))
            .await
            .unwrap();
        let board_id_str = res_create.0["id"].as_str().unwrap();
        let board_id = Uuid::parse_str(board_id_str).unwrap();
        assert_eq!(res_create.0["name"], "Test Board");

        let res_list = list(State(state.clone())).await.unwrap();
        assert!(res_list.0["count"].as_i64().unwrap() >= 1);

        let res_get = get(Path(board_id), State(state.clone())).await.unwrap();
        assert_eq!(res_get.0["name"], "Test Board");

        let res_get_404 = get(Path(Uuid::now_v7()), State(state.clone())).await;
        assert!(res_get_404.is_err());
        assert_eq!(res_get_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let body_update = json!({
            "name": "Updated Test Board",
            "description": "Updated desc",
            "parameters": { "layout": "columns" }
        });
        let res_update = update(Path(board_id), State(state.clone()), Json(body_update))
            .await
            .unwrap();
        assert_eq!(res_update.0["name"], "Updated Test Board");

        let body_clone = json!({
            "name": "Cloned Board"
        });
        let res_clone = clone(Path(board_id), State(state.clone()), Json(body_clone))
            .await
            .unwrap();
        let clone_id_str = res_clone.0["id"].as_str().unwrap();
        let clone_id = Uuid::parse_str(clone_id_str).unwrap();
        assert_eq!(res_clone.0["name"], "Cloned Board");

        let res_clone_404 =
            clone(Path(Uuid::now_v7()), State(state.clone()), Json(json!({}))).await;
        assert!(res_clone_404.is_err());
        assert_eq!(res_clone_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_clone_default = clone(Path(board_id), State(state.clone()), Json(json!({}))).await;
        assert!(res_clone_default.is_ok());
        let _ = delete(
            Path(Uuid::parse_str(res_clone_default.unwrap().0["id"].as_str().unwrap()).unwrap()),
            State(state.clone()),
        )
        .await;

        let res_clone_toolong = clone(
            Path(board_id),
            State(state.clone()),
            Json(json!({ "name": "x".repeat(300) })),
        )
        .await;
        assert!(res_clone_toolong.is_err());
        assert_eq!(
            res_clone_toolong.unwrap_err().0,
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let res_del_1 = delete(Path(board_id), State(state.clone())).await.unwrap();
        assert_eq!(res_del_1.0["ok"], true);

        let res_del_2 = delete(Path(clone_id), State(state.clone())).await.unwrap();
        assert_eq!(res_del_2.0["ok"], true);

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

        assert!(list(State(err_state.clone())).await.is_err());
        assert!(
            create(State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
        assert!(get(Path(board_id), State(err_state.clone())).await.is_err());
        assert!(
            update(Path(board_id), State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
        assert!(
            delete(Path(board_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            clone(Path(board_id), State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
    }
}
