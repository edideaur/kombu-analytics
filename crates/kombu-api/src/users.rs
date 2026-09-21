#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::{AdminUser, AuthUser, hash_password};
use crate::router::AppState;

pub async fn list(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT user_id as id, username, role, created_at as "createdAt"
            FROM "user"
            WHERE deleted_at IS NULL
            ORDER BY username ASC
            LIMIT 100
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

    Ok(Json(rows))
}

pub async fn create(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let username = body["username"].as_str().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Username required" })),
        )
    })?;
    let password_plain = body["password"].as_str().unwrap_or("password123");
    let password_hash = hash_password(password_plain)?;
    let role = body["role"].as_str().unwrap_or("user");
    let user_id = Uuid::now_v7();

    sqlx::query(
        r#"
        INSERT INTO "user" (user_id, username, password, role, created_at)
        VALUES ($1, $2, $3, $4, NOW())
        "#,
    )
    .bind(user_id)
    .bind(username.to_lowercase())
    .bind(password_hash)
    .bind(role)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "id": user_id,
        "username": username,
        "role": role
    })))
}

pub async fn get(
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" && auth.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT user_id as id, username, role, created_at as "createdAt"
            FROM "user"
            WHERE user_id = $1 AND deleted_at IS NULL
        ) t
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some(u) => Ok(Json(u)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )),
    }
}

pub async fn update(
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" && auth.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    let role = if auth.role == "admin" {
        body["role"].as_str()
    } else {
        None
    };
    let username = body["username"].as_str();
    let password_hash = if let Some(p) = body["password"].as_str() {
        if p.is_empty() {
            None
        } else {
            Some(hash_password(p)?)
        }
    } else {
        None
    };

    sqlx::query(
        r#"
        UPDATE "user"
        SET
            role = COALESCE($2, role),
            username = COALESCE($3, username),
            password = COALESCE($4, password),
            updated_at = NOW()
        WHERE user_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(role)
    .bind(username.map(str::to_lowercase))
    .bind(password_hash)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(auth, Path(user_id), State(state)).await
}

pub async fn delete(
    admin: AdminUser,
    Path(user_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if admin.0.user_id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Cannot delete current logged-in user" })),
        ));
    }

    sqlx::query(
        r#"
        UPDATE "user"
        SET deleted_at = NOW()
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
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

pub async fn websites(
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" && auth.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
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
            WHERE (user_id = $1 OR user_id IS NULL) AND deleted_at IS NULL
            ORDER BY name ASC
            LIMIT 100
        ) t
        "#,
    )
    .bind(user_id)
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

pub async fn teams(
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" && auth.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                t.team_id as id,
                t.name,
                tu.role,
                t.created_at as "createdAt"
            FROM "team_user" tu
            JOIN "team" t ON t.team_id = tu.team_id AND t.deleted_at IS NULL
            WHERE tu.user_id = $1
            ORDER BY t.name ASC
            LIMIT 100
        ) t
        "#,
    )
    .bind(user_id)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_users_full_management() {
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

        let admin_id = Uuid::now_v7();
        let admin_auth = AuthUser {
            user_id: admin_id,
            username: "admin_u".into(),
            role: "admin".into(),
        };
        let admin_wrapper = AdminUser(admin_auth.clone());

        let res_no_uname = create(
            admin_wrapper.clone(),
            State(state.clone()),
            Json(json!({ "password": "Password123!" })),
        )
        .await;
        assert!(res_no_uname.is_err());
        assert_eq!(res_no_uname.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_short_pw = create(
            admin_wrapper.clone(),
            State(state.clone()),
            Json(json!({
                "username": "short_pw_user",
                "password": "pwd"
            })),
        )
        .await;
        assert!(res_short_pw.is_err());
        assert_eq!(res_short_pw.unwrap_err().0, StatusCode::BAD_REQUEST);

        let uname = format!("user_{}", Uuid::now_v7().simple());
        let res_create = create(
            admin_wrapper.clone(),
            State(state.clone()),
            Json(json!({
                "username": uname,
                "password": "Password123!",
                "role": "user"
            })),
        )
        .await
        .unwrap();
        let created_user_id = res_create.0["id"].as_str().unwrap();
        let target_user_id = Uuid::parse_str(created_user_id).unwrap();

        let target_auth = AuthUser {
            user_id: target_user_id,
            username: uname.clone(),
            role: "user".into(),
        };

        let stranger_auth = AuthUser {
            user_id: Uuid::now_v7(),
            username: "stranger".into(),
            role: "user".into(),
        };

        let res_list = list(admin_wrapper.clone(), State(state.clone()))
            .await
            .unwrap();
        assert!(!res_list.0.as_array().unwrap().is_empty());

        let res_get_admin = get(
            admin_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_get_admin.0["username"], uname);

        let res_get_self = get(
            target_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_get_self.0["username"], uname);

        let res_get_forbidden = get(
            stranger_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await;
        assert!(res_get_forbidden.is_err());
        assert_eq!(res_get_forbidden.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_get_404 = get(
            admin_auth.clone(),
            Path(Uuid::now_v7()),
            State(state.clone()),
        )
        .await;
        assert!(res_get_404.is_err());
        assert_eq!(res_get_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_up_forbidden = update(
            stranger_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
            Json(json!({ "username": "hacked" })),
        )
        .await;
        assert!(res_up_forbidden.is_err());
        assert_eq!(res_up_forbidden.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_up_short = update(
            target_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
            Json(json!({ "password": "short" })),
        )
        .await;
        assert!(res_up_short.is_err());
        assert_eq!(res_up_short.unwrap_err().0, StatusCode::BAD_REQUEST);

        let updated_uname = format!("{uname}_up");
        let res_up_self = update(
            target_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
            Json(json!({
                "username": updated_uname,
                "password": "NewPassword123!"
            })),
        )
        .await
        .unwrap();
        assert_eq!(res_up_self.0["username"], updated_uname);

        let res_up_empty_pw = update(
            target_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
            Json(json!({
                "password": ""
            })),
        )
        .await;
        assert!(res_up_empty_pw.is_ok());

        let res_site_forbidden = websites(
            stranger_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await;
        assert!(res_site_forbidden.is_err());
        assert_eq!(res_site_forbidden.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_site_ok = websites(
            admin_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_site_ok.0["page"], 1);

        let res_team_forbidden = teams(
            stranger_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await;
        assert!(res_team_forbidden.is_err());
        assert_eq!(res_team_forbidden.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_team_ok = teams(
            target_auth.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_team_ok.0["page"], 1);

        let res_del_self =
            delete(admin_wrapper.clone(), Path(admin_id), State(state.clone())).await;
        assert!(res_del_self.is_err());
        assert_eq!(res_del_self.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_del_target = delete(
            admin_wrapper.clone(),
            Path(target_user_id),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_del_target.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(target_user_id)
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
            list(admin_wrapper.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            create(
                admin_wrapper.clone(),
                State(err_state.clone()),
                Json(json!({ "username": "fail" }))
            )
            .await
            .is_err()
        );
        assert!(
            get(
                admin_auth.clone(),
                Path(target_user_id),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            update(
                admin_auth.clone(),
                Path(target_user_id),
                State(err_state.clone()),
                Json(json!({ "username": "fail" }))
            )
            .await
            .is_err()
        );
        assert!(
            delete(
                admin_wrapper.clone(),
                Path(target_user_id),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            websites(
                admin_auth.clone(),
                Path(target_user_id),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            teams(
                admin_auth.clone(),
                Path(target_user_id),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
    }
}
