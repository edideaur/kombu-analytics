#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use totp_rs::{Algorithm, Secret, Totp};
use uuid::Uuid;

use crate::auth::{AuthUser, Claims, MaybeAuthUser, verify_password};
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct ConfirmBody {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyBody {
    pub token: Option<String>,
    #[serde(rename = "backupCode")]
    pub backup_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DisableBody {
    pub password: Option<String>,
    pub token: Option<String>,
}

pub fn create_totp(secret_base32: &str, username: &str) -> Result<Totp, String> {
    let secret = Secret::try_from_base32(secret_base32)
        .map_err(|e| format!("Invalid base32 secret: {e}"))?;

    #[allow(deprecated)]
    Ok(Totp::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.as_bytes().to_vec(),
        Some("Kombu Analytics".to_string()),
        username.to_string(),
    )
    .unwrap_or_default())
}

pub async fn status(
    auth: MaybeAuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let Some(user) = auth.0 else {
        return Ok(Json(json!({
            "isEnabled": false,
            "isRequired": false,
            "requiredReason": null
        })));
    };

    let is_enabled = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT COALESCE(is_enabled, false)
        FROM "two_factor_auth"
        WHERE user_id = $1
        "#,
    )
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or(false);

    let is_user_required = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT two_factor_required
        FROM "user"
        WHERE user_id = $1
        "#,
    )
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or(false);

    let is_team_required = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM "team" t
            JOIN "team_user" tu ON tu.team_id = t.team_id
            WHERE tu.user_id = $1 AND t.two_factor_required = true AND t.deleted_at IS NULL
        )
        "#,
    )
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    let (is_required, reason) = if is_user_required {
        (true, Some("user"))
    } else if is_team_required {
        (true, Some("team"))
    } else {
        (false, None)
    };

    Ok(Json(json!({
        "isEnabled": is_enabled,
        "isRequired": is_required,
        "requiredReason": reason
    })))
}

pub async fn initiate(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    initiate_with_secret(auth, State(state), Secret::generate().to_base32()).await
}

pub async fn initiate_with_secret(
    auth: AuthUser,
    State(state): State<AppState>,
    secret_base32: String,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let existing = sqlx::query_scalar::<_, bool>(
        r#"SELECT is_enabled FROM "two_factor_auth" WHERE user_id = $1"#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or(false);

    if existing {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": "two-factor-error-already-enabled",
                "error": "2FA is already enabled"
            })),
        ));
    }

    let totp = create_totp(&secret_base32, &auth.username).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
    })?;

    let qr_code_data_url = totp.to_qr_base64().unwrap_or_default();
    let qr_data_uri = format!("data:image/png;base64,{qr_code_data_url}");
    let record_id = Uuid::now_v7().to_string();

    sqlx::query(
        r#"
        INSERT INTO "two_factor_auth" (id, user_id, secret, is_enabled, created_at, updated_at)
        VALUES ($1, $2, $3, false, NOW(), NOW())
        ON CONFLICT ("user_id") DO UPDATE
        SET secret = EXCLUDED.secret, is_enabled = false, updated_at = NOW()
        "#,
    )
    .bind(record_id)
    .bind(auth.user_id)
    .bind(&secret_base32)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "secret": secret_base32,
        "manualKey": secret_base32,
        "qrCode": qr_data_uri,
        "qrCodeDataUrl": qr_data_uri
    })))
}

pub async fn confirm(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<ConfirmBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as::<_, (String, bool)>(
        r#"SELECT secret, is_enabled FROM "two_factor_auth" WHERE user_id = $1"#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((secret_base32, is_enabled)) = row else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": "two-factor-error-no-pending-setup",
                "error": "No pending 2FA setup found"
            })),
        ));
    };

    if is_enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "2FA is already enabled" })),
        ));
    }

    let totp = create_totp(&secret_base32, &auth.username).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
    })?;

    let token_clean = body.token.trim();
    if totp.check_current(token_clean).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "code": "two-factor-error-invalid-code",
                "error": "Invalid verification code"
            })),
        ));
    }

    let mut plaintext_codes = Vec::new();
    let mut hashed_codes = Vec::new();
    for _ in 0..10 {
        let code = format!("{:08x}", Uuid::now_v7().as_u128() & 0xffff_ffff);
        let hash = bcrypt::hash(&code, 10).unwrap_or_default();
        plaintext_codes.push(code);
        hashed_codes.push(hash);
    }

    store_confirmed_two_factor(&state.pool, auth.user_id, &hashed_codes)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!({
        "backupCodes": plaintext_codes
    })))
}

pub(crate) async fn store_confirmed_two_factor(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    hashed_codes: &[String],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE "two_factor_auth"
        SET is_enabled = true, updated_at = NOW()
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(r#"DELETE FROM "two_factor_backup_code" WHERE user_id = $1"#)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    for hash in hashed_codes {
        let code_id = Uuid::now_v7().to_string();
        sqlx::query(
            r#"
            INSERT INTO "two_factor_backup_code" (id, user_id, code_hash, used, created_at)
            VALUES ($1, $2, $3, false, NOW())
            "#,
        )
        .bind(code_id)
        .bind(user_id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(())
}

pub async fn cancel(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(
        r#"
        DELETE FROM "two_factor_auth"
        WHERE user_id = $1 AND is_enabled = false
        "#,
    )
    .bind(auth.user_id)
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

pub async fn disable(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<DisableBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT u.password, t.secret
        FROM "user" u
        JOIN "two_factor_auth" t ON t.user_id = u.user_id
        WHERE u.user_id = $1 AND t.is_enabled = true AND u.deleted_at IS NULL
        "#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((stored_password_hash, secret_base32)) = row else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "2FA is not enabled" })),
        ));
    };

    let mut verified = false;
    if let Some(pwd) = body.password {
        if verify_password(&pwd, &stored_password_hash) {
            verified = true;
        }
    }

    if !verified {
        if let Some(token) = body.token {
            if let Ok(totp) = create_totp(&secret_base32, &auth.username) {
                if totp.check_current(token.trim()).is_some() {
                    verified = true;
                }
            }
        }
    }

    if !verified {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid password or 2FA token" })),
        ));
    }

    remove_two_factor(&state.pool, auth.user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn remove_two_factor(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(r#"DELETE FROM "two_factor_auth" WHERE user_id = $1"#)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(r#"DELETE FROM "two_factor_backup_code" WHERE user_id = $1"#)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(())
}

pub async fn verify(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<VerifyBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok());
    let token = kombu_core::auth::bearer_token(auth_header).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing token" })),
        )
    })?;

    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let claims = jsonwebtoken::decode::<Claims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid token" })),
        )
    })?
    .claims;

    let user_id = claims.user_id;

    let client_ip = crate::rate_limit::extract_ip(&headers);
    let rate_key = format!("2fa_verify:{user_id}:{client_ip}");
    if let Err((status, _, err_body)) = state.rate_limiter.check(&rate_key, 5, 60) {
        return Err((status, err_body));
    }

    let two_factor = sqlx::query_scalar::<_, String>(
        r#"SELECT secret FROM "two_factor_auth" WHERE user_id = $1 AND is_enabled = true"#,
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(secret_base32) = two_factor else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "2FA is not enabled for this user" })),
        ));
    };

    let mut is_valid = false;

    if let Some(ref totp_code) = body.token {
        let code_clean = totp_code.trim();
        let already_used = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (SELECT 1 FROM "two_factor_otp_used" WHERE user_id = $1 AND otp = $2)"#,
        )
        .bind(user_id)
        .bind(code_clean)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if already_used {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Code already used" })),
            ));
        }

        if let Ok(totp) = create_totp(&secret_base32, &claims.username) {
            if totp.check_current(code_clean).is_some() {
                is_valid = true;
                let used_id = Uuid::now_v7().to_string();
                let expires_at = Utc::now() + Duration::minutes(5);
                let _ = sqlx::query(
                    r#"
                    INSERT INTO "two_factor_otp_used" (id, user_id, otp, expires_at)
                    VALUES ($1, $2, $3, $4)
                    "#,
                )
                .bind(used_id)
                .bind(user_id)
                .bind(code_clean)
                .bind(expires_at)
                .execute(&state.pool)
                .await;
            }
        }
    }

    if !is_valid {
        if let Some(ref backup_code) = body.backup_code {
            let unused_backup_codes = sqlx::query_as::<_, (String, String)>(
                r#"SELECT id, code_hash FROM "two_factor_backup_code" WHERE user_id = $1 AND used = false"#,
            )
            .bind(user_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

            for (code_id, code_hash) in unused_backup_codes {
                if verify_password(backup_code.trim(), &code_hash) {
                    is_valid = true;
                    let _ = sqlx::query(
                        r#"UPDATE "two_factor_backup_code" SET used = true WHERE id = $1"#,
                    )
                    .bind(code_id)
                    .execute(&state.pool)
                    .await;
                    break;
                }
            }
        }
    }

    if !is_valid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid 2FA code or backup code" })),
        ));
    }

    let full_claims = Claims {
        user_id,
        username: claims.username.clone(),
        role: claims.role.clone(),
        exp: (Utc::now() + Duration::days(7)).timestamp() as usize,
    };

    let session_token = crate::auth::encode_jwt_token(&full_claims, &secret).unwrap_or_default();

    Ok(Json(json!({
        "token": session_token,
        "user": {
            "id": user_id,
            "username": claims.username,
            "role": claims.role,
            "createdAt": Utc::now().to_rfc3339(),
            "isAdmin": claims.role == "admin",
            "teams": []
        }
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_two_factor_tx_failure_paths() {
        let admin = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();
        sqlx::query("DROP DATABASE IF EXISTS kombu_cov_2fa WITH (FORCE)")
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query("CREATE DATABASE kombu_cov_2fa")
            .execute(&admin)
            .await
            .unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect("postgres://kombu:kombu@localhost:5432/kombu_cov_2fa")
            .await
            .unwrap();
        let uid = Uuid::now_v7();
        let hashes = vec!["covhash".to_string()];

        assert!(store_confirmed_two_factor(&pool, uid, &hashes).await.is_err());

        sqlx::query(
            "CREATE TABLE two_factor_auth (user_id UUID NOT NULL, is_enabled BOOLEAN NOT NULL DEFAULT false, updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(store_confirmed_two_factor(&pool, uid, &hashes).await.is_err());

        sqlx::query("CREATE TABLE two_factor_backup_code (user_id UUID NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        assert!(store_confirmed_two_factor(&pool, uid, &hashes).await.is_err());

        sqlx::query("DROP TABLE two_factor_backup_code")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE two_factor_backup_code (id TEXT NOT NULL PRIMARY KEY, user_id UUID NOT NULL, code_hash TEXT NOT NULL, used BOOLEAN NOT NULL DEFAULT false, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE OR REPLACE FUNCTION cov_boom() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'covboom'; END; $$",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE CONSTRAINT TRIGGER cov_defer_ins AFTER INSERT ON two_factor_backup_code DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION cov_boom()",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(store_confirmed_two_factor(&pool, uid, &hashes).await.is_err());
        sqlx::query("DROP TRIGGER IF EXISTS cov_defer_ins ON two_factor_backup_code")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DROP TABLE two_factor_backup_code")
            .execute(&pool)
            .await
            .unwrap();
        assert!(remove_two_factor(&pool, uid).await.is_err());

        sqlx::query(
            "CREATE TABLE two_factor_backup_code (id TEXT NOT NULL PRIMARY KEY, user_id UUID NOT NULL, code_hash TEXT NOT NULL, used BOOLEAN NOT NULL DEFAULT false, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE CONSTRAINT TRIGGER cov_defer_del AFTER DELETE ON two_factor_backup_code DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION cov_boom()",
        )
        .execute(&pool)
        .await
        .unwrap();
        let seed_id = Uuid::now_v7().to_string();
        sqlx::query(r#"INSERT INTO "two_factor_backup_code" (id, user_id, code_hash) VALUES ($1, $2, 'x')"#)
            .bind(&seed_id)
            .bind(uid)
            .execute(&pool)
            .await
            .unwrap();
        assert!(remove_two_factor(&pool, uid).await.is_err());

        pool.close().await;
        sqlx::query("DROP DATABASE IF EXISTS kombu_cov_2fa WITH (FORCE)")
            .execute(&admin)
            .await
            .unwrap();
    }
    use axum::http::HeaderValue;

    #[test]
    fn test_create_totp_error() {
        assert!(create_totp("invalid base32 secret!", "user").is_err());
    }

    #[tokio::test]
    async fn test_two_factor_edge_cases() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let _ = sqlx::query(r#"DROP RULE IF EXISTS no_ins_test ON "two_factor_backup_code";"#)
            .execute(&pool)
            .await;
        let state = AppState {
            pool: pool.clone(),
            session_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            website_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            rate_limiter: crate::rate_limit::RateLimiter::new(),
            ingest_queue: crate::queue::IngestQueue::new(&pool, 16, 2),
            app_secret: std::sync::Arc::new("kombu-secret".into()),
        };

        let res_anon_status = status(MaybeAuthUser(None), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_anon_status.0["isEnabled"], false);
        assert_eq!(res_anon_status.0["isRequired"], false);

        let user_id = Uuid::now_v7();
        let username = format!("totp_edge_{}", user_id.simple());
        let password = "TestPassword123!";
        let pass_hash = crate::auth::hash_password(password).unwrap();

        sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role, two_factor_required, created_at, updated_at) VALUES ($1, $2, $3, 'user', true, NOW(), NOW())"#,
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
            role: "user".into(),
        };

        let res_req_status = status(
            MaybeAuthUser(Some(auth_user.clone())),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_req_status.0["isRequired"], true);
        assert_eq!(res_req_status.0["requiredReason"], "user");

        let team_u_id = Uuid::now_v7();
        let team_u_name = format!("totp_team_u_{}", team_u_id.simple());
        let team_id = Uuid::now_v7();
        let _ = sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role, two_factor_required, created_at, updated_at) VALUES ($1, $2, 'pass', 'user', false, NOW(), NOW())"#,
        )
        .bind(team_u_id)
        .bind(&team_u_name)
        .execute(&pool)
        .await;

        let _ = sqlx::query(
            r#"INSERT INTO "team" (team_id, name, access_code, two_factor_required, created_at, updated_at) VALUES ($1, '2FA Team', 'code_2fa', true, NOW(), NOW())"#,
        )
        .bind(team_id)
        .execute(&pool)
        .await;

        let _ = sqlx::query(
            r#"INSERT INTO "team_user" (team_user_id, team_id, user_id, role, created_at) VALUES ($1, $2, $3, 'team-member', NOW())"#,
        )
        .bind(Uuid::now_v7())
        .bind(team_id)
        .bind(team_u_id)
        .execute(&pool)
        .await;

        let team_auth = AuthUser {
            user_id: team_u_id,
            username: team_u_name,
            role: "user".into(),
        };

        let res_team_req_status = status(
            MaybeAuthUser(Some(team_auth.clone())),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_team_req_status.0["isRequired"], true);
        assert_eq!(res_team_req_status.0["requiredReason"], "team");

        let _ = sqlx::query(r#"DELETE FROM "team_user" WHERE team_id = $1"#).bind(team_id).execute(&pool).await;
        let _ = sqlx::query(r#"DELETE FROM "team" WHERE team_id = $1"#).bind(team_id).execute(&pool).await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#).bind(team_u_id).execute(&pool).await;

        let res_no_pending = confirm(
            auth_user.clone(),
            State(state.clone()),
            Json(ConfirmBody {
                token: "123456".into(),
            }),
        )
        .await;
        assert!(res_no_pending.is_err());
        assert_eq!(res_no_pending.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_init = initiate(auth_user.clone(), State(state.clone()))
            .await
            .unwrap();
        let secret = res_init.0["secret"].as_str().unwrap();
        assert!(!secret.is_empty());

        let res_bad_code = confirm(
            auth_user.clone(),
            State(state.clone()),
            Json(ConfirmBody {
                token: "000000".into(),
            }),
        )
        .await;
        assert!(res_bad_code.is_err());
        assert_eq!(res_bad_code.unwrap_err().0, StatusCode::BAD_REQUEST);

        let _ = sqlx::query(r#"UPDATE "two_factor_auth" SET secret = '???' WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let res_corrupt_confirm = confirm(
            auth_user.clone(),
            State(state.clone()),
            Json(ConfirmBody {
                token: "123456".into(),
            }),
        )
        .await;
        assert!(res_corrupt_confirm.is_err());
        assert_eq!(
            res_corrupt_confirm.unwrap_err().0,
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let _ = sqlx::query(r#"UPDATE "two_factor_auth" SET secret = $2 WHERE user_id = $1"#)
            .bind(user_id)
            .bind(secret)
            .execute(&pool)
            .await;

        let res_cancel = cancel(auth_user.clone(), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_cancel.0["ok"], true);

        let res_init_2 = initiate(auth_user.clone(), State(state.clone()))
            .await
            .unwrap();
        let secret_2 = res_init_2.0["secret"].as_str().unwrap().to_string();
        let totp = create_totp(&secret_2, &username).unwrap();
        let current_code = totp.generate_current().to_string();

        let res_confirm = confirm(
            auth_user.clone(),
            State(state.clone()),
            Json(ConfirmBody {
                token: current_code.clone(),
            }),
        )
        .await
        .unwrap();
        let backup_codes = res_confirm.0["backupCodes"].as_array().unwrap();
        assert_eq!(backup_codes.len(), 10);
        let first_backup = backup_codes[0].as_str().unwrap().to_string();

        let res_init_already = initiate(auth_user.clone(), State(state.clone())).await;
        assert!(res_init_already.is_err());
        assert_eq!(res_init_already.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_conf_already = confirm(
            auth_user.clone(),
            State(state.clone()),
            Json(ConfirmBody {
                token: current_code.clone(),
            }),
        )
        .await;
        assert!(res_conf_already.is_err());
        assert_eq!(res_conf_already.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_no_tok = verify(HeaderMap::new(), State(state.clone()), Json(VerifyBody { token: None, backup_code: None })).await;
        assert!(res_no_tok.is_err());
        assert_eq!(res_no_tok.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let mut bad_headers = HeaderMap::new();
        bad_headers.insert("authorization", HeaderValue::from_static("Bearer bad_token"));
        let res_bad_tok = verify(bad_headers, State(state.clone()), Json(VerifyBody { token: None, backup_code: None })).await;
        assert!(res_bad_tok.is_err());
        assert_eq!(res_bad_tok.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let secret_key = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
        let claims = Claims {
            user_id,
            username: username.clone(),
            role: "user".into(),
            exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
        };
        let token_jwt = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret_key.as_bytes()),
        )
        .unwrap();
        let make_headers = |ip: &str| {
            let mut h = HeaderMap::new();
            h.insert("authorization", HeaderValue::from_str(&format!("Bearer {token_jwt}")).unwrap());
            h.insert("x-forwarded-for", HeaderValue::from_str(ip).unwrap());
            h
        };
        let valid_headers = make_headers("10.0.0.1");

        let res_v_bad = verify(
            make_headers("10.0.0.2"),
            State(state.clone()),
            Json(VerifyBody {
                token: Some("999999".into()),
                backup_code: None,
            }),
        )
        .await;
        assert!(res_v_bad.is_err());
        assert_eq!(res_v_bad.unwrap_err().0, StatusCode::BAD_REQUEST);

        let valid_totp_code = totp.generate_current().to_string();
        let res_v_ok = verify(
            make_headers("10.0.0.3"),
            State(state.clone()),
            Json(VerifyBody {
                token: Some(valid_totp_code.clone()),
                backup_code: None,
            }),
        )
        .await
        .unwrap();
        assert!(res_v_ok.0["token"].is_string());

        let res_replay = verify(
            make_headers("10.0.0.4"),
            State(state.clone()),
            Json(VerifyBody {
                token: Some(valid_totp_code),
                backup_code: None,
            }),
        )
        .await;
        assert!(res_replay.is_err());
        assert_eq!(res_replay.unwrap_err().0, StatusCode::BAD_REQUEST);

        let _ = sqlx::query(r#"UPDATE "two_factor_auth" SET secret = '???' WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let res_v_corrupt = verify(
            make_headers("10.0.0.5"),
            State(state.clone()),
            Json(VerifyBody {
                token: Some("123456".into()),
                backup_code: None,
            }),
        )
        .await;
        assert!(res_v_corrupt.is_err());
        assert_eq!(res_v_corrupt.unwrap_err().0, StatusCode::BAD_REQUEST);

        let _ = sqlx::query(r#"UPDATE "two_factor_auth" SET secret = $2 WHERE user_id = $1"#)
            .bind(user_id)
            .bind(&secret_2)
            .execute(&pool)
            .await;

        let res_v_backup = verify(
            make_headers("10.0.0.6"),
            State(state.clone()),
            Json(VerifyBody {
                token: None,
                backup_code: Some(first_backup.clone()),
            }),
        )
        .await
        .unwrap();
        assert!(res_v_backup.0["token"].is_string());

        let res_v_backup_replay = verify(
            make_headers("10.0.0.7"),
            State(state.clone()),
            Json(VerifyBody {
                token: None,
                backup_code: Some(first_backup),
            }),
        )
        .await;
        assert!(res_v_backup_replay.is_err());
        assert_eq!(res_v_backup_replay.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_dis_bad = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: Some("WrongPassword123!".into()),
                token: Some("000000".into()),
            }),
        )
        .await;
        assert!(res_dis_bad.is_err());
        assert_eq!(res_dis_bad.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let res_dis_ok = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: Some(password.into()),
                token: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(res_dis_ok.0["ok"], true);

        let res_dis_already = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: Some(password.into()),
                token: None,
            }),
        )
        .await;
        assert!(res_dis_already.is_err());
        assert_eq!(res_dis_already.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_re_init = initiate(auth_user.clone(), State(state.clone())).await.unwrap();
        let re_secret = res_re_init.0["secret"].as_str().unwrap().to_string();
        let re_totp = create_totp(&re_secret, &username).unwrap();
        let re_code = re_totp.generate_current().to_string();
        let _ = confirm(auth_user.clone(), State(state.clone()), Json(ConfirmBody { token: re_code.clone() })).await.unwrap();

        let res_dis_none = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: None,
                token: None,
            }),
        )
        .await;
        assert!(res_dis_none.is_err());
        assert_eq!(res_dis_none.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let _ = sqlx::query(r#"UPDATE "two_factor_auth" SET secret = '???' WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let res_dis_corrupt_totp = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: None,
                token: Some("123456".into()),
            }),
        )
        .await;
        assert!(res_dis_corrupt_totp.is_err());
        assert_eq!(res_dis_corrupt_totp.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let _ = sqlx::query(r#"UPDATE "two_factor_auth" SET secret = $2 WHERE user_id = $1"#)
            .bind(user_id)
            .bind(&re_secret)
            .execute(&pool)
            .await;

        let res_dis_bad_token = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: None,
                token: Some("000000".into()),
            }),
        )
        .await;
        assert!(res_dis_bad_token.is_err());
        assert_eq!(res_dis_bad_token.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let _ = sqlx::query(r#"CREATE OR REPLACE RULE no_del_test AS ON DELETE TO "two_factor_auth" DO INSTEAD (SELECT 1/0);"#)
            .execute(&pool)
            .await;
        let res_dis_err = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: None,
                token: Some(re_totp.generate_current().to_string()),
            }),
        )
        .await;
        assert!(res_dis_err.is_err());
        assert_eq!(res_dis_err.unwrap_err().0, StatusCode::INTERNAL_SERVER_ERROR);
        let _ = sqlx::query(r#"DROP RULE IF EXISTS no_del_test ON "two_factor_auth";"#)
            .execute(&pool)
            .await;

        let res_dis_token = disable(
            auth_user.clone(),
            State(state.clone()),
            Json(DisableBody {
                password: None,
                token: Some(re_totp.generate_current().to_string()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(res_dis_token.0["ok"], true);

        let res_err_init = initiate(auth_user.clone(), State(state.clone())).await.unwrap();
        let err_secret = res_err_init.0["secret"].as_str().unwrap().to_string();
        let err_totp = create_totp(&err_secret, &username).unwrap();
        let err_code = err_totp.generate_current().to_string();

        let _ = sqlx::query(r#"CREATE OR REPLACE RULE no_ins_test AS ON INSERT TO "two_factor_backup_code" DO INSTEAD (SELECT 1/0);"#)
            .execute(&pool)
            .await;
        let res_conf_err = confirm(
            auth_user.clone(),
            State(state.clone()),
            Json(ConfirmBody { token: err_code }),
        )
        .await;
        assert!(res_conf_err.is_err());
        assert_eq!(res_conf_err.unwrap_err().0, StatusCode::INTERNAL_SERVER_ERROR);
        let _ = sqlx::query(r#"DROP RULE IF EXISTS no_ins_test ON "two_factor_backup_code";"#)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "two_factor_auth" WHERE user_id = $1"#).bind(user_id).execute(&pool).await;

        let other_u_id = Uuid::now_v7();
        let other_claims = Claims {
            user_id: other_u_id,
            username: "other_user".into(),
            role: "user".into(),
            exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
        };
        let other_jwt = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &other_claims,
            &jsonwebtoken::EncodingKey::from_secret(secret_key.as_bytes()),
        )
        .unwrap();
        let mut other_headers = HeaderMap::new();
        other_headers.insert("authorization", HeaderValue::from_str(&format!("Bearer {other_jwt}")).unwrap());

        let res_v_not_enabled = verify(
            other_headers.clone(),
            State(state.clone()),
            Json(VerifyBody {
                token: Some("123456".into()),
                backup_code: None,
            }),
        )
        .await;
        assert!(res_v_not_enabled.is_err());
        assert_eq!(res_v_not_enabled.unwrap_err().0, StatusCode::BAD_REQUEST);

        for _ in 0..6 {
            let _ = verify(
                other_headers.clone(),
                State(state.clone()),
                Json(VerifyBody {
                    token: Some("123456".into()),
                    backup_code: None,
                }),
            )
            .await;
        }

        let _ = sqlx::query(r#"DELETE FROM "two_factor_otp_used" WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "two_factor_backup_code" WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "two_factor_auth" WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;

        let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        closed_pool.close().await;
        let err_state = AppState {
            pool: closed_pool.clone(),
            session_cache: state.session_cache.clone(),
            website_cache: state.website_cache.clone(),
            rate_limiter: state.rate_limiter.clone(),
            ingest_queue: state.ingest_queue.clone(),
            app_secret: state.app_secret.clone(),
        };

        assert!(status(MaybeAuthUser(Some(auth_user.clone())), State(err_state.clone())).await.is_ok());
        assert!(initiate(auth_user.clone(), State(err_state.clone())).await.is_err());
        assert!(confirm(auth_user.clone(), State(err_state.clone()), Json(ConfirmBody { token: "123456".into() })).await.is_err());
        assert!(cancel(auth_user.clone(), State(err_state.clone())).await.is_err());
        assert!(disable(auth_user.clone(), State(err_state.clone()), Json(DisableBody { password: Some("pwd".into()), token: None })).await.is_err());
        assert!(verify(valid_headers, State(err_state.clone()), Json(VerifyBody { token: Some("123456".into()), backup_code: None })).await.is_err());
        assert!(store_confirmed_two_factor(&closed_pool, user_id, &["hash".into()]).await.is_err());
        assert!(remove_two_factor(&closed_pool, user_id).await.is_err());
        let res_init_bad = initiate_with_secret(auth_user.clone(), State(state.clone()), "invalid!base32!".into()).await;
        assert!(res_init_bad.is_err());
        assert_eq!(res_init_bad.unwrap_err().0, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
