#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::router::AppState;

pub fn hash_invitation_token(token: &str) -> String {
    let mut hasher = Sha512::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn list(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = if auth.role == "admin" {
        sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
                SELECT team_id as id, name, access_code as "accessCode", created_at as "createdAt"
                FROM "team"
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
                SELECT t.team_id as id, t.name, t.access_code as "accessCode", t.created_at as "createdAt"
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
    }
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(rows))
}

pub async fn create(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Name required" })),
        )
    })?;
    let team_id = Uuid::now_v7();
    let access_code = format!("team_{}", Uuid::now_v7().simple());

    sqlx::query(
        r#"
        INSERT INTO "team" (team_id, name, access_code, created_at)
        VALUES ($1, $2, $3, NOW())
        "#,
    )
    .bind(team_id)
    .bind(name)
    .bind(&access_code)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let team_user_id = Uuid::now_v7();
    let _ = sqlx::query(
        r#"
        INSERT INTO "team_user" (team_user_id, team_id, user_id, role, created_at)
        VALUES ($1, $2, $3, 'owner', NOW())
        "#,
    )
    .bind(team_user_id)
    .bind(team_id)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await;

    Ok(Json(json!({
        "id": team_id,
        "name": name,
        "accessCode": access_code
    })))
}

pub async fn join(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let access_code = body["accessCode"].as_str().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "accessCode required" })),
        )
    })?;

    let row = sqlx::query_as::<_, (Uuid, String)>(
        r#"SELECT team_id, name FROM "team" WHERE access_code = $1 AND deleted_at IS NULL"#,
    )
    .bind(access_code)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some((id, name)) => Ok(Json(json!({ "id": id, "name": name }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Team not found" })),
        )),
    }
}

pub async fn get(
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT team_id as id, name, access_code as "accessCode", created_at as "createdAt"
            FROM "team"
            WHERE team_id = $1 AND deleted_at IS NULL
        ) t
        "#,
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

    match row {
        Some(t) => Ok(Json(t)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Team not found" })),
        )),
    }
}

pub async fn update(
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str();

    sqlx::query(
        r#"
        UPDATE "team"
        SET
            name = COALESCE($2, name),
            updated_at = NOW()
        WHERE team_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(team_id)
    .bind(name)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(team_id), State(state)).await
}

pub async fn delete(
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" {
        let is_owner = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM "team_user"
                WHERE team_id = $1 AND user_id = $2 AND role = 'owner'
            )
            "#,
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if !is_owner {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Forbidden: team owner privileges required" })),
            ));
        }
    }

    sqlx::query(
        r#"
        UPDATE "team"
        SET deleted_at = NOW()
        WHERE team_id = $1
        "#,
    )
    .bind(team_id)
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

pub async fn users(
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                tu.team_user_id as id,
                tu.user_id as "userId",
                tu.role,
                u.username,
                tu.created_at as "createdAt"
            FROM "team_user" tu
            JOIN "user" u ON u.user_id = tu.user_id
            WHERE tu.team_id = $1 AND u.deleted_at IS NULL
            ORDER BY tu.created_at ASC
        ) t
        "#,
    )
    .bind(team_id)
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

pub async fn add_user(
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" {
        let can_manage = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM "team_user"
                WHERE team_id = $1 AND user_id = $2 AND role IN ('owner', 'admin')
            )
            "#,
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if !can_manage {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Forbidden: team management privileges required" })),
            ));
        }
    }

    let user_id = body["userId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "userId required" })),
            )
        })?;
    let role = body["role"].as_str().unwrap_or("team-member");
    let team_user_id = Uuid::now_v7();

    sqlx::query(
        r#"
        INSERT INTO "team_user" (team_user_id, team_id, user_id, role, created_at)
        VALUES ($1, $2, $3, $4, NOW())
        "#,
    )
    .bind(team_user_id)
    .bind(team_id)
    .bind(user_id)
    .bind(role)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(
        json!({ "id": team_user_id, "userId": user_id, "role": role }),
    ))
}

pub async fn delete_user(
    auth: AuthUser,
    Path((team_id, user_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" && auth.user_id != user_id {
        let can_manage = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM "team_user"
                WHERE team_id = $1 AND user_id = $2 AND role IN ('owner', 'admin')
            )
            "#,
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if !can_manage {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Forbidden: team management privileges required" })),
            ));
        }
    }

    sqlx::query(r#"DELETE FROM "team_user" WHERE team_id = $1 AND user_id = $2"#)
        .bind(team_id)
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
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
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
            WHERE team_id = $1 AND deleted_at IS NULL
            ORDER BY name ASC
            LIMIT 100
        ) t
        "#,
    )
    .bind(team_id)
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

pub async fn boards(
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                board_id as id,
                name,
                description,
                type,
                parameters,
                user_id as "userId",
                team_id as "teamId",
                created_at as "createdAt"
            FROM "board"
            WHERE team_id = $1
            ORDER BY created_at DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(team_id)
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

pub async fn pixels(
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                pixel_id as id,
                name,
                slug,
                user_id as "userId",
                team_id as "teamId",
                created_at as "createdAt"
            FROM "pixel"
            WHERE team_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(team_id)
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

pub async fn links(
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                link_id as id,
                name,
                url,
                slug,
                user_id as "userId",
                team_id as "teamId",
                created_at as "createdAt"
            FROM "link"
            WHERE team_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(team_id)
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

pub async fn create_invitation(
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" {
        let can_manage = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM "team_user"
                WHERE team_id = $1 AND user_id = $2 AND role IN ('owner', 'admin')
            )
            "#,
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if !can_manage {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Forbidden: team management privileges required" })),
            ));
        }
    }

    let role = body["role"].as_str().unwrap_or("team-member");
    let hours = body["expiresInHours"].as_i64().unwrap_or(168);
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(hours);

    let raw_token = format!(
        "inv_{}_{}",
        Uuid::now_v7().simple(),
        Uuid::now_v7().simple()
    );
    let token_hash = hash_invitation_token(&raw_token);
    let invitation_id = Uuid::now_v7();

    sqlx::query(
        r#"
        INSERT INTO "team_invitation" (
            invitation_id, team_id, role, token_hash, created_by, expires_at, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6, NOW())
        "#,
    )
    .bind(invitation_id)
    .bind(team_id)
    .bind(role)
    .bind(&token_hash)
    .bind(auth.user_id)
    .bind(expires_at)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "id": invitation_id,
        "teamId": team_id,
        "role": role,
        "token": raw_token,
        "expiresAt": expires_at.to_rfc3339(),
        "createdAt": chrono::Utc::now().to_rfc3339()
    })))
}

pub async fn list_invitations(
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" {
        let is_member = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (SELECT 1 FROM "team_user" WHERE team_id = $1 AND user_id = $2)"#,
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if !is_member {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Forbidden: not a team member" })),
            ));
        }
    }

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                invitation_id as id,
                team_id as "teamId",
                role,
                created_by as "createdBy",
                expires_at as "expiresAt",
                accepted_at as "acceptedAt",
                accepted_by as "acceptedBy",
                revoked_at as "revokedAt",
                created_at as "createdAt"
            FROM "team_invitation"
            WHERE team_id = $1
            ORDER BY created_at DESC
        ) t
        "#,
    )
    .bind(team_id)
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

pub async fn revoke_invitation(
    auth: AuthUser,
    Path((team_id, invitation_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if auth.role != "admin" {
        let can_manage = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM "team_user"
                WHERE team_id = $1 AND user_id = $2 AND role IN ('owner', 'admin')
            )
            "#,
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if !can_manage {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Forbidden: team management privileges required" })),
            ));
        }
    }

    sqlx::query(
        r#"UPDATE "team_invitation" SET revoked_at = NOW() WHERE invitation_id = $1 AND team_id = $2"#
    )
    .bind(invitation_id)
    .bind(team_id)
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

pub async fn get_invitation_by_token(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token_hash = hash_invitation_token(&token);
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                i.invitation_id as id,
                i.team_id as "teamId",
                t.name as "teamName",
                i.role,
                i.expires_at as "expiresAt",
                (i.expires_at < NOW() OR i.revoked_at IS NOT NULL OR i.accepted_at IS NOT NULL) as "isExpired"
            FROM "team_invitation" i
            JOIN "team" t ON t.team_id = i.team_id
            WHERE i.token_hash = $1
        ) t
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some(val) => Ok(Json(val)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Invitation not found" })),
        )),
    }
}

pub async fn accept_invitation_by_token(
    auth: AuthUser,
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token_hash = hash_invitation_token(&token);

    let row = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            chrono::DateTime<chrono::Utc>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT invitation_id, team_id, role, expires_at, accepted_at, revoked_at
        FROM "team_invitation"
        WHERE token_hash = $1
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let Some((invitation_id, team_id, role, expires_at, accepted_at, revoked_at)) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Invitation not found" })),
        ));
    };

    if revoked_at.is_some() {
        return Err((
            StatusCode::GONE,
            Json(json!({ "error": "Invitation has been revoked" })),
        ));
    }
    if accepted_at.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invitation already accepted" })),
        ));
    }
    if expires_at < chrono::Utc::now() {
        return Err((
            StatusCode::GONE,
            Json(json!({ "error": "Invitation has expired" })),
        ));
    }

    let existing_role = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT role FROM "team_user" WHERE team_id = $1 AND user_id = $2"#,
    )
    .bind(team_id)
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or_default();

    if existing_role.is_none() {
        let team_user_id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO "team_user" (team_user_id, team_id, user_id, role, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            "#,
        )
        .bind(team_user_id)
        .bind(team_id)
        .bind(auth.user_id)
        .bind(&role)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    }

    let _ = sqlx::query(
        r#"
        UPDATE "team_invitation"
        SET accepted_at = NOW(), accepted_by = $2
        WHERE invitation_id = $1
        "#,
    )
    .bind(invitation_id)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await;

    Ok(Json(json!({ "ok": true, "teamId": team_id })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_teams_full_lifecycle_and_rbac() {
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

        let _ =
            sqlx::query(r#"ALTER TABLE "team_user" DROP CONSTRAINT IF EXISTS "check_test_fail""#)
                .execute(&pool)
                .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE username LIKE 'team_%'"#)
            .execute(&pool)
            .await;

        let owner_id = Uuid::now_v7();
        let owner_auth = AuthUser {
            user_id: owner_id,
            username: format!("team_owner_{}", owner_id.simple()),
            role: "user".into(),
        };

        let member_id = Uuid::now_v7();
        let member_auth = AuthUser {
            user_id: member_id,
            username: format!("team_member_{}", member_id.simple()),
            role: "user".into(),
        };

        let outsider_id = Uuid::now_v7();
        let outsider_auth = AuthUser {
            user_id: outsider_id,
            username: format!("team_outsider_{}", outsider_id.simple()),
            role: "user".into(),
        };

        let admin_id = Uuid::now_v7();
        let admin_auth = AuthUser {
            user_id: admin_id,
            username: format!("team_admin_{}", admin_id.simple()),
            role: "admin".into(),
        };

        for u in [&owner_auth, &member_auth, &outsider_auth, &admin_auth] {
            let _ = sqlx::query(
                r#"INSERT INTO "user" (user_id, username, password, role, created_at, updated_at) VALUES ($1, $2, 'pass', $3, NOW(), NOW())"#,
            )
            .bind(u.user_id)
            .bind(&u.username)
            .bind(&u.role)
            .execute(&pool)
            .await;
        }

        let res_no_name = create(owner_auth.clone(), State(state.clone()), Json(json!({}))).await;
        assert!(res_no_name.is_err());
        assert_eq!(res_no_name.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_create = create(
            owner_auth.clone(),
            State(state.clone()),
            Json(json!({ "name": "Engineering Team" })),
        )
        .await
        .unwrap();
        let team_id_str = res_create.0["id"].as_str().unwrap();
        let team_id = Uuid::parse_str(team_id_str).unwrap();
        let access_code = res_create.0["accessCode"].as_str().unwrap().to_string();

        let res_user_list = list(owner_auth.clone(), State(state.clone()))
            .await
            .unwrap();
        assert!(!res_user_list.0.as_array().unwrap().is_empty());

        let res_admin_list = list(admin_auth.clone(), State(state.clone()))
            .await
            .unwrap();
        assert!(!res_admin_list.0.as_array().unwrap().is_empty());

        let res_join_bad = join(State(state.clone()), Json(json!({}))).await;
        assert!(res_join_bad.is_err());
        assert_eq!(res_join_bad.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_join_404 = join(
            State(state.clone()),
            Json(json!({ "accessCode": "unknown_code" })),
        )
        .await;
        assert!(res_join_404.is_err());
        assert_eq!(res_join_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_join_ok = join(
            State(state.clone()),
            Json(json!({ "accessCode": access_code })),
        )
        .await
        .unwrap();
        assert_eq!(res_join_ok.0["id"], team_id_str);

        let res_get = get(Path(team_id), State(state.clone())).await.unwrap();
        assert_eq!(res_get.0["name"], "Engineering Team");

        let res_get_404 = get(Path(Uuid::now_v7()), State(state.clone())).await;
        assert!(res_get_404.is_err());
        assert_eq!(res_get_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_up = update(
            Path(team_id),
            State(state.clone()),
            Json(json!({ "name": "Core Engineering Team" })),
        )
        .await
        .unwrap();
        assert_eq!(res_up.0["name"], "Core Engineering Team");

        let res_add_forbidden = add_user(
            outsider_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({ "userId": member_id.to_string(), "role": "team-member" })),
        )
        .await;
        assert!(res_add_forbidden.is_err());
        assert_eq!(res_add_forbidden.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_add_no_user = add_user(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({ "role": "team-member" })),
        )
        .await;
        assert!(res_add_no_user.is_err());
        assert_eq!(res_add_no_user.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_add_ok = add_user(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({ "userId": member_id.to_string(), "role": "team-member" })),
        )
        .await
        .unwrap();
        assert_eq!(res_add_ok.0["userId"], member_id.to_string());

        let res_users = users(Path(team_id), State(state.clone())).await.unwrap();
        assert!(res_users.0.as_array().unwrap().len() >= 2);

        let res_websites = websites(Path(team_id), State(state.clone())).await.unwrap();
        assert_eq!(res_websites.0["page"], 1);

        let res_boards = boards(Path(team_id), State(state.clone())).await.unwrap();
        assert_eq!(res_boards.0["page"], 1);

        let res_pixels = pixels(Path(team_id), State(state.clone())).await.unwrap();
        assert_eq!(res_pixels.0["page"], 1);

        let res_links = links(Path(team_id), State(state.clone())).await.unwrap();
        assert_eq!(res_links.0["page"], 1);

        let res_del_user_forbid = delete_user(
            outsider_auth.clone(),
            Path((team_id, member_id)),
            State(state.clone()),
        )
        .await;
        assert!(res_del_user_forbid.is_err());
        assert_eq!(res_del_user_forbid.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_del_self = delete_user(
            member_auth.clone(),
            Path((team_id, member_id)),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_del_self.0["ok"], true);

        let _ = add_user(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({ "userId": member_id.to_string(), "role": "team-member" })),
        )
        .await;
        let res_del_by_owner = delete_user(
            owner_auth.clone(),
            Path((team_id, member_id)),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_del_by_owner.0["ok"], true);

        assert!(!hash_invitation_token("test-inv-token").is_empty());

        let res_inv_forbid = create_invitation(
            outsider_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({})),
        )
        .await;
        assert!(res_inv_forbid.is_err());
        assert_eq!(res_inv_forbid.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_inv1 = create_invitation(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({ "role": "team-member", "expiresInHours": 24 })),
        )
        .await
        .unwrap();
        let inv1_id: Uuid = res_inv1.0["id"].as_str().unwrap().parse().unwrap();
        let inv1_token = res_inv1.0["token"].as_str().unwrap().to_string();

        let res_list_inv_forbid =
            list_invitations(outsider_auth.clone(), Path(team_id), State(state.clone())).await;
        assert!(res_list_inv_forbid.is_err());

        let res_list_inv =
            list_invitations(owner_auth.clone(), Path(team_id), State(state.clone()))
                .await
                .unwrap();
        assert!(!res_list_inv.0.as_array().unwrap().is_empty());

        let res_list_inv_admin =
            list_invitations(admin_auth.clone(), Path(team_id), State(state.clone()))
                .await
                .unwrap();
        assert!(!res_list_inv_admin.0.as_array().unwrap().is_empty());

        let res_inv_admin = create_invitation(
            admin_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({})),
        )
        .await
        .unwrap();
        let inv_admin_id: Uuid = res_inv_admin.0["id"].as_str().unwrap().parse().unwrap();
        let _inv_admin_token = res_inv_admin.0["token"].as_str().unwrap().to_string();

        let _ = sqlx::query(r#"ALTER TABLE "team_user" ADD CONSTRAINT "check_test_fail" CHECK (role != 'fail_role')"#)
            .execute(&pool)
            .await;

        let res_inv_fail = create_invitation(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({ "role": "fail_role" })),
        )
        .await
        .unwrap();
        let inv_fail_token = res_inv_fail.0["token"].as_str().unwrap().to_string();

        let res_accept_fail = accept_invitation_by_token(
            outsider_auth.clone(),
            Path(inv_fail_token),
            State(state.clone()),
        )
        .await;
        assert!(res_accept_fail.is_err());
        assert_eq!(
            res_accept_fail.unwrap_err().0,
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let _ =
            sqlx::query(r#"ALTER TABLE "team_user" DROP CONSTRAINT IF EXISTS "check_test_fail""#)
                .execute(&pool)
                .await;

        let res_rev_admin = revoke_invitation(
            admin_auth.clone(),
            Path((team_id, inv_admin_id)),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_rev_admin.0["ok"], true);

        let res_get_inv_404 =
            get_invitation_by_token(Path("nonexistent_inv_token".into()), State(state.clone()))
                .await;
        assert!(res_get_inv_404.is_err());
        assert_eq!(res_get_inv_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_get_inv_ok =
            get_invitation_by_token(Path(inv1_token.clone()), State(state.clone()))
                .await
                .unwrap();
        assert_eq!(res_get_inv_ok.0["teamId"], team_id.to_string());

        let res_accept = accept_invitation_by_token(
            outsider_auth.clone(),
            Path(inv1_token.clone()),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_accept.0["ok"], true);

        let res_accept_again = accept_invitation_by_token(
            outsider_auth.clone(),
            Path(inv1_token.clone()),
            State(state.clone()),
        )
        .await;
        assert!(res_accept_again.is_err());
        assert_eq!(res_accept_again.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_inv_existing = create_invitation(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({})),
        )
        .await
        .unwrap();
        let inv_existing_token = res_inv_existing.0["token"].as_str().unwrap().to_string();
        let res_accept_existing = accept_invitation_by_token(
            owner_auth.clone(),
            Path(inv_existing_token),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_accept_existing.0["ok"], true);

        let res_inv2 = create_invitation(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({})),
        )
        .await
        .unwrap();
        let inv2_id: Uuid = res_inv2.0["id"].as_str().unwrap().parse().unwrap();
        let inv2_token = res_inv2.0["token"].as_str().unwrap().to_string();

        let res_rev_forbid = revoke_invitation(
            outsider_auth.clone(),
            Path((team_id, inv2_id)),
            State(state.clone()),
        )
        .await;
        assert!(res_rev_forbid.is_err());

        let res_rev_ok = revoke_invitation(
            owner_auth.clone(),
            Path((team_id, inv2_id)),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_rev_ok.0["ok"], true);

        let res_accept_revoked = accept_invitation_by_token(
            outsider_auth.clone(),
            Path(inv2_token),
            State(state.clone()),
        )
        .await;
        assert!(res_accept_revoked.is_err());
        assert_eq!(res_accept_revoked.unwrap_err().0, StatusCode::GONE);

        let res_inv3 = create_invitation(
            owner_auth.clone(),
            Path(team_id),
            State(state.clone()),
            Json(json!({ "expiresInHours": -2 })),
        )
        .await
        .unwrap();
        let inv3_token = res_inv3.0["token"].as_str().unwrap().to_string();

        let res_accept_expired = accept_invitation_by_token(
            outsider_auth.clone(),
            Path(inv3_token),
            State(state.clone()),
        )
        .await;
        assert!(res_accept_expired.is_err());
        assert_eq!(res_accept_expired.unwrap_err().0, StatusCode::GONE);

        let res_accept_404 = accept_invitation_by_token(
            outsider_auth.clone(),
            Path("ghost_token_xyz".into()),
            State(state.clone()),
        )
        .await;
        assert!(res_accept_404.is_err());
        assert_eq!(res_accept_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_del_team_forbid =
            delete(outsider_auth.clone(), Path(team_id), State(state.clone())).await;
        assert!(res_del_team_forbid.is_err());
        assert_eq!(res_del_team_forbid.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_del_team_ok = delete(owner_auth.clone(), Path(team_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_del_team_ok.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "team_invitation" WHERE team_id = $1"#)
            .bind(team_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "team" WHERE team_id = $1"#)
            .bind(team_id)
            .execute(&pool)
            .await;
        for u in [&owner_auth, &member_auth, &outsider_auth, &admin_auth] {
            let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
                .bind(u.user_id)
                .execute(&pool)
                .await;
        }

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
            list(owner_auth.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            list(admin_auth.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            create(
                owner_auth.clone(),
                State(err_state.clone()),
                Json(json!({ "name": "Team Fail" }))
            )
            .await
            .is_err()
        );
        assert!(
            join(
                State(err_state.clone()),
                Json(json!({ "accessCode": "code" }))
            )
            .await
            .is_err()
        );
        assert!(get(Path(team_id), State(err_state.clone())).await.is_err());
        assert!(
            update(
                Path(team_id),
                State(err_state.clone()),
                Json(json!({ "name": "Team Fail" }))
            )
            .await
            .is_err()
        );
        assert!(
            delete(owner_auth.clone(), Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            delete(admin_auth.clone(), Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            add_user(
                owner_auth.clone(),
                Path(team_id),
                State(err_state.clone()),
                Json(json!({ "userId": member_id.to_string(), "role": "team-member" }))
            )
            .await
            .is_err()
        );
        assert!(
            add_user(
                admin_auth.clone(),
                Path(team_id),
                State(err_state.clone()),
                Json(json!({ "userId": member_id.to_string(), "role": "team-member" }))
            )
            .await
            .is_err()
        );
        assert!(
            delete_user(
                owner_auth.clone(),
                Path((team_id, member_id)),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            delete_user(
                admin_auth.clone(),
                Path((team_id, member_id)),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            users(Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            websites(Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            boards(Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            pixels(Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            links(Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            create_invitation(
                owner_auth.clone(),
                Path(team_id),
                State(err_state.clone()),
                Json(json!({}))
            )
            .await
            .is_err()
        );
        assert!(
            create_invitation(
                admin_auth.clone(),
                Path(team_id),
                State(err_state.clone()),
                Json(json!({}))
            )
            .await
            .is_err()
        );
        assert!(
            list_invitations(owner_auth.clone(), Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            list_invitations(admin_auth.clone(), Path(team_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            revoke_invitation(
                owner_auth.clone(),
                Path((team_id, inv1_id)),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            revoke_invitation(
                admin_auth.clone(),
                Path((team_id, inv1_id)),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            get_invitation_by_token(Path("any".into()), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            accept_invitation_by_token(
                owner_auth.clone(),
                Path("any".into()),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
    }
}
