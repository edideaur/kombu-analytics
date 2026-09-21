#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::AdminUser;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct TwoFactorRequiredPayload {
    pub required: bool,
}

pub async fn users(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                u.user_id as id,
                u.username,
                u.role,
                u.created_at as "createdAt",
                COUNT(w.website_id)::bigint as "websiteCount"
            FROM "user" u
            LEFT JOIN "website" w ON w.user_id = u.user_id AND w.deleted_at IS NULL
            WHERE u.deleted_at IS NULL
            GROUP BY u.user_id, u.username, u.role, u.created_at
            ORDER BY u.created_at DESC
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

    let count = rows.as_array().map_or(0, |a| a.len());

    Ok(Json(json!({
        "data": rows,
        "count": count,
        "page": 1,
        "pageSize": 100
    })))
}

pub async fn teams(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                t.team_id as id,
                t.name,
                t.access_code as "accessCode",
                t.created_at as "createdAt",
                COUNT(DISTINCT tu.user_id)::bigint as "memberCount",
                COUNT(DISTINCT w.website_id)::bigint as "websiteCount"
            FROM "team" t
            LEFT JOIN "team_user" tu ON tu.team_id = t.team_id
            LEFT JOIN "website" w ON w.team_id = t.team_id AND w.deleted_at IS NULL
            WHERE t.deleted_at IS NULL
            GROUP BY t.team_id, t.name, t.access_code, t.created_at
            ORDER BY t.created_at DESC
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

    let count = rows.as_array().map_or(0, |a| a.len());

    Ok(Json(json!({
        "data": rows,
        "count": count,
        "page": 1,
        "pageSize": 100
    })))
}

pub async fn websites(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                w.website_id as id,
                w.name,
                w.domain,
                w.created_at as "createdAt",
                u.username as "owner"
            FROM "website" w
            LEFT JOIN "user" u ON u.user_id = w.user_id
            WHERE w.deleted_at IS NULL
            ORDER BY w.created_at DESC
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

    let count = rows.as_array().map_or(0, |a| a.len());

    Ok(Json(json!({
        "data": rows,
        "count": count,
        "page": 1,
        "pageSize": 100
    })))
}

pub async fn two_factor_global(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<TwoFactorRequiredPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let val_str = if payload.required { "true" } else { "false" };
    sqlx::query(
        r#"
        INSERT INTO app_setting (key, value)
        VALUES ('twoFactorRequiredGlobal', $1)
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value
        "#,
    )
    .bind(val_str)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "ok": true,
        "required": payload.required
    })))
}

pub async fn user_two_factor(
    _admin: AdminUser,
    Path(user_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE(is_enabled, false) FROM two_factor_auth WHERE user_id = $1",
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
    Ok(Json(json!({ "isEnabled": row.unwrap_or(false) })))
}

pub async fn update_user_two_factor(
    _admin: AdminUser,
    Path(user_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(payload): Json<TwoFactorRequiredPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query("UPDATE \"user\" SET two_factor_required = $1 WHERE user_id = $2")
        .bind(payload.required)
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(json!({ "ok": true, "required": payload.required })))
}

pub async fn team_two_factor(
    _admin: AdminUser,
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE(two_factor_required, false) FROM \"team\" WHERE team_id = $1",
    )
    .bind(team_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!({ "required": row.unwrap_or(false) })))
}

pub async fn update_team_two_factor(
    _admin: AdminUser,
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(payload): Json<TwoFactorRequiredPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query("UPDATE \"team\" SET two_factor_required = $1 WHERE team_id = $2")
        .bind(payload.required)
        .bind(team_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(json!({ "ok": true, "required": payload.required })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::auth::AuthUser;

    #[tokio::test]
    async fn test_admin_endpoints_full() {
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

        let admin = AdminUser(AuthUser {
            user_id: Uuid::now_v7(),
            username: "admin_tester".into(),
            role: "admin".into(),
        });

        let res_users = users(admin.clone(), State(state.clone())).await;
        assert!(res_users.is_ok());

        let res_teams = teams(admin.clone(), State(state.clone())).await;
        assert!(res_teams.is_ok());

        let res_websites = websites(admin.clone(), State(state.clone())).await;
        assert!(res_websites.is_ok());

        let res_tfg_true = two_factor_global(
            admin.clone(),
            State(state.clone()),
            Json(TwoFactorRequiredPayload { required: true }),
        )
        .await;
        assert!(res_tfg_true.is_ok());
        let res_tfg_false = two_factor_global(
            admin.clone(),
            State(state.clone()),
            Json(TwoFactorRequiredPayload { required: false }),
        )
        .await;
        assert!(res_tfg_false.is_ok());

        let test_user_id = Uuid::now_v7();
        let _ = sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role) VALUES ($1, $2, $3, 'user')"#,
        )
        .bind(test_user_id)
        .bind(format!("admin_tf_u_{test_user_id}"))
        .bind("hash")
        .execute(&pool)
        .await;

        let test_team_id = Uuid::now_v7();
        let _ =
            sqlx::query(r#"INSERT INTO "team" (team_id, name, access_code) VALUES ($1, $2, $3)"#)
                .bind(test_team_id)
                .bind(format!("admin_tf_t_{test_team_id}"))
                .bind(format!("code_{test_team_id}"))
                .execute(&pool)
                .await;

        let res_u_tf =
            user_two_factor(admin.clone(), Path(test_user_id), State(state.clone())).await;
        assert!(res_u_tf.is_ok());

        let res_upd_u_tf = update_user_two_factor(
            admin.clone(),
            Path(test_user_id),
            State(state.clone()),
            Json(TwoFactorRequiredPayload { required: true }),
        )
        .await;
        assert!(res_upd_u_tf.is_ok());

        let res_t_tf =
            team_two_factor(admin.clone(), Path(test_team_id), State(state.clone())).await;
        assert!(res_t_tf.is_ok());

        let res_upd_t_tf = update_team_two_factor(
            admin.clone(),
            Path(test_team_id),
            State(state.clone()),
            Json(TwoFactorRequiredPayload { required: true }),
        )
        .await;
        assert!(res_upd_t_tf.is_ok());

        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(test_user_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "team" WHERE team_id = $1"#)
            .bind(test_team_id)
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
            users(admin.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            teams(admin.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            websites(admin.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            two_factor_global(
                admin.clone(),
                State(err_state.clone()),
                Json(TwoFactorRequiredPayload { required: true })
            )
            .await
            .is_err()
        );
        let res_u_tf_err =
            user_two_factor(admin.clone(), Path(test_user_id), State(err_state.clone())).await;
        assert!(res_u_tf_err.is_err());
        let res_upd_u_tf_err = update_user_two_factor(
            admin.clone(),
            Path(test_user_id),
            State(err_state.clone()),
            Json(TwoFactorRequiredPayload { required: true }),
        )
        .await;
        assert!(res_upd_u_tf_err.is_err());
        let res_t_tf_err =
            team_two_factor(admin.clone(), Path(test_team_id), State(err_state.clone())).await;
        assert!(res_t_tf_err.is_err());
        let res_upd_t_tf_err = update_team_two_factor(
            admin.clone(),
            Path(test_team_id),
            State(err_state.clone()),
            Json(TwoFactorRequiredPayload { required: true }),
        )
        .await;
        assert!(res_upd_t_tf_err.is_err());
    }
}
