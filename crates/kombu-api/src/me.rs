#![forbid(unsafe_code)]

use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::{AuthUser, hash_password, verify_password};
use crate::router::AppState;

pub async fn get(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"SELECT user_id, username, role FROM "user" WHERE user_id = $1 AND deleted_at IS NULL"#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    if let Some((id, username, role)) = row {
        let is_admin = role == "admin";
        Ok(Json(json!({
            "user": {
                "id": id,
                "username": username,
                "role": role,
                "isAdmin": is_admin
            }
        })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        ))
    }
}

pub async fn websites(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = if auth.role == "admin" {
        sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
                SELECT
                    website_id as id,
                    name,
                    domain,
                    user_id as "userId",
                    team_id as "teamId",
                    created_at as "createdAt"
                FROM "website"
                WHERE deleted_at IS NULL
                ORDER BY name ASC
                LIMIT 100
            ) t
            "#,
        )
        .fetch_one(&state.pool)
        .await
    } else {
        sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
                SELECT
                    w.website_id as id,
                    w.name,
                    w.domain,
                    w.user_id as "userId",
                    w.team_id as "teamId",
                    w.created_at as "createdAt"
                FROM "website" w
                LEFT JOIN "team_user" tu ON tu.team_id = w.team_id AND tu.user_id = $1
                WHERE w.deleted_at IS NULL AND (w.user_id = $1 OR tu.user_id IS NOT NULL)
                ORDER BY w.name ASC
                LIMIT 100
            ) t
            "#,
        )
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
    }
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

pub async fn teams(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                t.team_id as id,
                t.name,
                t.access_code as "accessCode",
                tu.role,
                t.created_at as "createdAt"
            FROM "team" t
            JOIN "team_user" tu ON tu.team_id = t.team_id AND tu.user_id = $1
            WHERE t.deleted_at IS NULL
            ORDER BY t.name ASC
            LIMIT 100
        ) t
        "#,
    )
    .bind(auth.user_id)
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

pub async fn password(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let current_password = body["currentPassword"].as_str().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "currentPassword required" })),
        )
    })?;

    let new_password = body["newPassword"].as_str().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "newPassword required" })),
        )
    })?;

    let current_hash = sqlx::query_scalar::<_, String>(
        r#"SELECT password FROM "user" WHERE user_id = $1 AND deleted_at IS NULL"#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
    })?;

    if !verify_password(current_password, &current_hash) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Current password is incorrect" })),
        ));
    }

    let hashed_new = hash_password(new_password)?;

    sqlx::query(
        r#"
        UPDATE "user"
        SET password = $2, updated_at = NOW()
        WHERE user_id = $1
        "#,
    )
    .bind(auth.user_id)
    .bind(hashed_new)
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_me_full_endpoints() {
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

        let user_id = Uuid::now_v7();
        let username = format!("me_{}", user_id.simple());
        let initial_pass = "InitialPass123!";
        let pass_hash = hash_password(initial_pass).unwrap();

        sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role, created_at, updated_at) VALUES ($1, $2, $3, 'user', NOW(), NOW())"#,
        )
        .bind(user_id)
        .bind(&username)
        .bind(&pass_hash)
        .execute(&pool)
        .await
        .unwrap();

        let auth_user = AuthUser {
            user_id,
            username: username.clone(),
            role: "user".to_string(),
        };

        let res_me = get(auth_user.clone(), State(state.clone())).await.unwrap();
        assert_eq!(res_me.0["user"]["username"], username);
        assert_eq!(res_me.0["user"]["isAdmin"], false);

        let res_me_404 = get(
            AuthUser {
                user_id: Uuid::now_v7(),
                username: "ghost".into(),
                role: "user".into(),
            },
            State(state.clone()),
        )
        .await;
        assert!(res_me_404.is_err());
        assert_eq!(res_me_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_user_sites = websites(auth_user.clone(), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_user_sites.0["count"], 0);

        let admin_user = AuthUser {
            user_id,
            username: username.clone(),
            role: "admin".to_string(),
        };
        let res_admin_sites = websites(admin_user.clone(), State(state.clone()))
            .await
            .unwrap();
        assert!(res_admin_sites.0["count"].as_i64().unwrap() >= 0);

        let res_teams = teams(auth_user.clone(), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_teams.0["count"], 0);

        let res_no_cur = password(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({ "newPassword": "NewPassword123!" })),
        )
        .await;
        assert!(res_no_cur.is_err());
        assert_eq!(res_no_cur.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_no_new = password(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({ "currentPassword": initial_pass })),
        )
        .await;
        assert!(res_no_new.is_err());
        assert_eq!(res_no_new.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_wrong_cur = password(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({
                "currentPassword": "WrongPassword123!",
                "newPassword": "NewPassword123!"
            })),
        )
        .await;
        assert!(res_wrong_cur.is_err());
        assert_eq!(res_wrong_cur.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_short_new = password(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({
                "currentPassword": initial_pass,
                "newPassword": "short"
            })),
        )
        .await;
        assert!(res_short_new.is_err());
        assert_eq!(res_short_new.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_ghost_pw = password(
            AuthUser {
                user_id: Uuid::now_v7(),
                username: "ghost".into(),
                role: "user".into(),
            },
            State(state.clone()),
            Json(json!({
                "currentPassword": initial_pass,
                "newPassword": "NewPassword123!"
            })),
        )
        .await;
        assert!(res_ghost_pw.is_err());
        assert_eq!(res_ghost_pw.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_ok_pw = password(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({
                "currentPassword": initial_pass,
                "newPassword": "UpdatedPassword123!"
            })),
        )
        .await
        .unwrap();
        assert_eq!(res_ok_pw.0["ok"], true);

        let constraint_query = format!(
            r#"ALTER TABLE "user" ADD CONSTRAINT test_check_pw CHECK (user_id != '{user_id}' OR length(password) < 10) NOT VALID"#
        );
        let res_alter = sqlx::query(&constraint_query).execute(&pool).await;
        assert!(res_alter.is_ok());
        let res_fail_pw = password(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({
                "currentPassword": "UpdatedPassword123!",
                "newPassword": "AnotherPassword123!"
            })),
        )
        .await;
        assert!(res_fail_pw.is_err());
        let _ = sqlx::query(r#"ALTER TABLE "user" DROP CONSTRAINT IF EXISTS test_check_pw"#)
            .execute(&pool)
            .await;

        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(user_id)
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
            get(auth_user.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            websites(auth_user.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            websites(admin_user, State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            teams(auth_user.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            password(
                auth_user,
                State(err_state.clone()),
                Json(json!({ "currentPassword": "p", "newPassword": "p" }))
            )
            .await
            .is_err()
        );
    }
}
