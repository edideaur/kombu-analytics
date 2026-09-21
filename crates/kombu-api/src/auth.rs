#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
};
use base64::Engine;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
    pub totp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub exp: usize,
}

#[must_use]
pub fn get_token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = kombu_core::auth::bearer_token(Some(auth_header)) {
            return Some(token.clone());
        }
    }
    if let Some(cookie_header) = headers.get("cookie").and_then(|v| v.to_str().ok()) {
        for pair in cookie_header.split(';') {
            let mut parts = pair.trim().splitn(2, '=');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                if k == "auth-token" || k == "umami.auth" {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[must_use]
pub fn get_claims_from_headers(headers: &HeaderMap) -> Option<Claims> {
    let token = get_token_from_headers(headers)?;
    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()?;
    Some(token_data.claims)
}

#[must_use]
pub fn get_user_id_from_headers(headers: &HeaderMap) -> Option<Uuid> {
    get_claims_from_headers(headers).map(|c| c.user_id)
}

#[must_use]
pub fn determine_sso_role(count_users: i64) -> String {
    if count_users == 0 {
        "admin".to_string()
    } else {
        std::env::var("OIDC_DEFAULT_ROLE").unwrap_or_else(|_| "view-only".to_string())
    }
}

pub fn hash_password(password: &str) -> Result<String, (StatusCode, Json<Value>)> {
    hash_password_with_cost(password, 10)
}

pub fn hash_password_with_cost(
    password: &str,
    cost: u32,
) -> Result<String, (StatusCode, Json<Value>)> {
    if password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Password must be at least 8 characters long" })),
        ));
    }
    bcrypt::hash(password, cost).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Password hashing failed: {e}") })),
        )
    })
}

#[must_use]
pub fn verify_password(password: &str, hash: &str) -> bool {
    if hash.starts_with("$2") {
        bcrypt::verify(password, hash).unwrap_or(false)
    } else {
        false
    }
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
}

impl<S> axum::extract::FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        if let Some(claims) = get_claims_from_headers(&parts.headers) {
            if claims.role == "partial" {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Two-factor authentication required" })),
                ));
            }
            Ok(AuthUser {
                user_id: claims.user_id,
                username: claims.username,
                role: claims.role,
            })
        } else {
            Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Unauthorized" })),
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

impl<S> axum::extract::FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<Value>);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        if auth.role != "admin" {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Forbidden: admin access required" })),
            ));
        }
        Ok(AdminUser(auth))
    }
}

#[derive(Debug, Clone)]
pub struct MaybeAuthUser(pub Option<AuthUser>);

impl<S> axum::extract::FromRequestParts<S> for MaybeAuthUser
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let user = get_claims_from_headers(&parts.headers).and_then(|claims| {
            if claims.role == "partial" {
                None
            } else {
                Some(AuthUser {
                    user_id: claims.user_id,
                    username: claims.username,
                    role: claims.role,
                })
            }
        });
        Ok(MaybeAuthUser(user))
    }
}

pub async fn login(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let client_ip = crate::rate_limit::extract_ip(&headers);
    let rate_key = format!("login:{client_ip}");
    if let Err((status, _, json_err)) = state.rate_limiter.check(&rate_key, 10, 60) {
        return Err((status, json_err));
    }

    let username_lower = body.username.to_lowercase();
    let row = sqlx::query_as::<_, (Uuid, String, String, String)>(
        r#"
        SELECT user_id, username, password, role
        FROM "user"
        WHERE username = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(&username_lower)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let Some((user_id, username, password_hash, role)) = row else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid username or password" })),
        ));
    };

    let valid = verify_password(&body.password, &password_hash);
    if !valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid username or password" })),
        ));
    }

    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());

    let (secret_base32, two_factor_enabled): (Option<String>, bool) =
        sqlx::query_as::<_, (String, bool)>(
            r#"SELECT secret, is_enabled FROM "two_factor_auth" WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .map_or((None, false), |(sec, en)| (Some(sec), en));

    if two_factor_enabled {
        let mut totp_verified = false;
        if let (Some(sec), Some(code)) = (secret_base32, body.totp.as_deref()) {
            if let Ok(totp) = crate::two_factor::create_totp(&sec, &username) {
                if totp.check_current(code.trim()).is_some() {
                    totp_verified = true;
                }
            }
        }

        if !totp_verified {
            let partial_claims = Claims {
                user_id,
                username: username.clone(),
                role: "partial".to_string(),
                exp: (chrono::Utc::now() + chrono::Duration::minutes(5)).timestamp() as usize,
            };

            let partial_token = encode_jwt_token(&partial_claims, &secret).unwrap_or_default();

            return Ok(Json(json!({
                "twoFactorRequired": true,
                "token": partial_token,
                "user": {
                    "id": user_id,
                    "username": username,
                    "role": role,
                }
            })));
        }
    }

    let token = create_user_token(user_id, &username, &role).unwrap_or_default();

    let is_admin = role == "admin";

    Ok(Json(json!({
        "token": token,
        "user": {
            "id": user_id,
            "username": username,
            "role": role,
            "createdAt": chrono::Utc::now().to_rfc3339(),
            "isAdmin": is_admin,
            "teams": []
        }
    })))
}

pub fn encode_jwt_token<T: serde::Serialize>(
    claims: &T,
    secret: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })
}

pub fn create_user_token(
    user_id: Uuid,
    username: &str,
    role: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let claims = Claims {
        user_id,
        username: username.to_string(),
        role: role.to_string(),
        exp: (chrono::Utc::now() + chrono::Duration::days(7)).timestamp() as usize,
    };

    encode_jwt_token(&claims, &secret)
}

pub async fn verify(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok());
    let token = kombu_core::auth::bearer_token(auth_header).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing token" })),
        )
    })?;

    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid token" })),
        )
    })?;

    let claims = token_data.claims;

    let db_user = sqlx::query_as::<_, (String, String)>(
        r#"SELECT username, role FROM "user" WHERE user_id = $1 AND deleted_at IS NULL"#,
    )
    .bind(claims.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let (username, role) = match db_user {
        Some((u, r)) => (u, r),
        None => (claims.username, claims.role),
    };

    let is_admin = role == "admin";

    Ok(Json(json!({
        "id": claims.user_id,
        "username": username,
        "role": role,
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "isAdmin": is_admin,
        "teams": []
    })))
}

pub async fn logout(State(_state): State<AppState>) -> Json<Value> {
    Json(json!({ "ok": true }))
}

pub async fn subscription() -> Json<Value> {
    Json(json!({
        "isPro": true,
        "isBusiness": true,
        "isNoBilling": true,
        "hasSubscription": true,
        "unlimitedWebsites": true
    }))
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct SsoLoginPayload {
    pub provider: Option<String>,
    pub token: Option<String>,
    #[serde(rename = "idToken")]
    pub id_token: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
}

pub fn sso_config_with_lookup(get_var: &dyn Fn(&str) -> Option<String>) -> Json<Value> {
    let oidc_client_id = get_var("OIDC_CLIENT_ID");
    let oidc_issuer = get_var("OIDC_ISSUER_URL");
    let auth_header = get_var("AUTH_HEADER");

    let enabled = oidc_client_id.is_some() || oidc_issuer.is_some() || auth_header.is_some();
    let provider = if oidc_issuer.is_some() {
        "oidc"
    } else if auth_header.is_some() {
        "header"
    } else {
        "sso"
    };

    let button_label =
        get_var("SSO_BUTTON_LABEL").unwrap_or_else(|| "Sign in with SSO".to_string());

    Json(json!({
        "enabled": enabled,
        "provider": provider,
        "url": "/api/auth/sso",
        "buttonLabel": button_label
    }))
}

pub async fn sso_config() -> Json<Value> {
    sso_config_with_lookup(&|k| std::env::var(k).ok())
}

pub async fn sso(
    headers: HeaderMap,
    State(state): State<AppState>,
    body: Option<Json<SsoLoginPayload>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sso_login(headers, State(state), body).await
}

pub async fn sso_login(
    headers: HeaderMap,
    State(state): State<AppState>,
    body: Option<Json<SsoLoginPayload>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let custom_auth_header = std::env::var("AUTH_HEADER").ok();
    let effective_auth_header = custom_auth_header.as_deref().or_else(|| {
        if headers.contains_key("x-remote-user") {
            Some("x-remote-user")
        } else {
            None
        }
    });
    let header_user = effective_auth_header
        .and_then(|h| headers.get(h))
        .and_then(|v| v.to_str().ok())
        .map(str::trim);

    let payload = body.map(|b| b.0).unwrap_or_default();

    let username_from_token = payload
        .id_token
        .as_deref()
        .or(payload.token.as_deref())
        .and_then(|tok| {
            let mut parts = tok.split('.');
            let _hdr: Option<&str> = parts.next();
            let payload_b64 = parts.next()?;
            let decoded = base64::prelude::BASE64_STANDARD_NO_PAD
                .decode(payload_b64)
                .or_else(|_| base64::prelude::BASE64_STANDARD.decode(payload_b64))
                .ok()?;
            let v: Value = serde_json::from_slice(&decoded).ok()?;
            v.get("preferred_username")
                .or_else(|| v.get("email"))
                .or_else(|| v.get("sub"))
                .and_then(|s| s.as_str())
                .map(str::to_string)
        });

    let effective_username = header_user.map(str::to_string).or(username_from_token);

    let Some(username) = effective_username else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Missing or invalid SSO credentials" })),
        ));
    };

    let provider = payload.provider.unwrap_or_else(|| "sso".to_string());
    let subject = username.clone();

    let existing_user_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT user_id FROM "user_external_auth"
        WHERE provider = $1 AND subject = $2
        "#,
    )
    .bind(&provider)
    .bind(&subject)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (user_id, role) = if let Some(uid) = existing_user_id {
        let role_row = sqlx::query_scalar::<_, String>(
            r#"SELECT role FROM "user" WHERE user_id = $1 AND deleted_at IS NULL"#,
        )
        .bind(uid)
        .fetch_optional(&state.pool)
        .await;
        let role = if let Ok(Some(r)) = role_row {
            r
        } else {
            "view-only".to_string()
        };
        (uid, role)
    } else {
        let user_row = sqlx::query_as::<_, (Uuid, String)>(
            r#"SELECT user_id, role FROM "user" WHERE username = $1 AND deleted_at IS NULL"#,
        )
        .bind(&username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        let (uid, r) = if let Some((u, r)) = user_row {
            (u, r)
        } else {
            let new_uid = Uuid::now_v7();
            let count_users = sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "user" WHERE deleted_at IS NULL"#,
            )
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

            let default_role = determine_sso_role(count_users);

            let dummy_hash = bcrypt::hash(Uuid::now_v7().to_string(), 10).unwrap_or_default();

            let _ = sqlx::query(
                r#"
                INSERT INTO "user" (user_id, username, password, role, created_at, updated_at)
                VALUES ($1, $2, $3, $4, NOW(), NOW())
                ON CONFLICT (username) DO NOTHING
                "#,
            )
            .bind(new_uid)
            .bind(&username)
            .bind(&dummy_hash)
            .bind(&default_role)
            .execute(&state.pool)
            .await;

            (new_uid, default_role)
        };

        let _ = sqlx::query(
            r#"
            INSERT INTO "user_external_auth" (id, user_id, provider, subject, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            ON CONFLICT (provider, subject) DO NOTHING
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(uid)
        .bind(&provider)
        .bind(&subject)
        .execute(&state.pool)
        .await;

        (uid, r)
    };

    let token = create_user_token(user_id, &username, &role).unwrap_or_default();
    let is_admin = role == "admin";

    Ok(Json(json!({
        "token": token,
        "user": {
            "id": user_id,
            "username": username,
            "role": role,
            "createdAt": chrono::Utc::now().to_rfc3339(),
            "isAdmin": is_admin,
            "teams": []
        }
    })))
}

#[derive(Debug, Deserialize, Default)]
pub struct SsoCallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
}

pub async fn sso_callback(
    Query(params): Query<SsoCallbackParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if let Some(code) = params.code {
        Ok(Json(json!({
            "status": "callback_received",
            "code": code
        })))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Missing authorization code" })),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::extract::FromRequestParts;
    use axum::http::{HeaderValue, Request};
    use base64::Engine;

    fn make_token(user_id: Uuid, username: &str, role: &str) -> String {
        let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
        let claims = Claims {
            user_id,
            username: username.to_string(),
            role: role.to_string(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_password_hashing_and_verification() {
        assert!(hash_password("short").is_err());
        assert!(hash_password_with_cost("validpassword", 32).is_err());
        let hash = hash_password("ValidPass123!").unwrap();
        assert!(verify_password("ValidPass123!", &hash));
        assert!(!verify_password("WrongPass123!", &hash));
        assert!(!verify_password("ValidPass123!", "plain_text"));
    }

    #[test]
    fn test_headers_token_extraction() {
        let mut headers = HeaderMap::new();
        assert_eq!(get_token_from_headers(&headers), None);
        assert!(get_claims_from_headers(&headers).is_none());
        assert_eq!(get_user_id_from_headers(&headers), None);

        let uid = Uuid::now_v7();
        let tok = make_token(uid, "testuser", "admin");
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {tok}")).unwrap(),
        );
        assert_eq!(get_token_from_headers(&headers), Some(tok.clone()));
        let claims = get_claims_from_headers(&headers).unwrap();
        assert_eq!(claims.user_id, uid);
        assert_eq!(claims.username, "testuser");
        assert_eq!(claims.role, "admin");
        assert_eq!(get_user_id_from_headers(&headers), Some(uid));

        let mut cookie_headers = HeaderMap::new();
        cookie_headers.insert(
            "cookie",
            HeaderValue::from_str(&format!("other=1; auth-token={tok}; foo=bar")).unwrap(),
        );
        assert_eq!(get_token_from_headers(&cookie_headers), Some(tok.clone()));

        let mut umami_headers = HeaderMap::new();
        umami_headers.insert(
            "cookie",
            HeaderValue::from_str(&format!("umami.auth={tok}")).unwrap(),
        );
        assert_eq!(get_token_from_headers(&umami_headers), Some(tok));

        let mut basic_headers = HeaderMap::new();
        basic_headers.insert("authorization", HeaderValue::from_static("Basic xyz"));
        assert_eq!(get_token_from_headers(&basic_headers), None);

        let mut invalid_jwt_headers = HeaderMap::new();
        invalid_jwt_headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer not-a-valid-jwt"),
        );
        assert!(get_claims_from_headers(&invalid_jwt_headers).is_none());

        let mut malformed_cookie_headers = HeaderMap::new();
        malformed_cookie_headers.insert(
            "cookie",
            HeaderValue::from_static("malformed_cookie_no_equal; other=123"),
        );
        assert_eq!(get_token_from_headers(&malformed_cookie_headers), None);

        assert_eq!(determine_sso_role(0), "admin");
        assert_eq!(determine_sso_role(1), "view-only");
    }

    #[tokio::test]
    async fn test_auth_extractors() {
        let uid = Uuid::now_v7();
        let admin_tok = make_token(uid, "admin", "admin");
        let user_tok = make_token(uid, "normal", "user");
        let partial_tok = make_token(uid, "partial_u", "partial");

        let req = Request::builder()
            .header("authorization", format!("Bearer {user_tok}"))
            .body(())
            .unwrap();
        let (mut parts, ()) = req.into_parts();
        let auth = AuthUser::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(auth.user_id, uid);
        assert_eq!(auth.role, "user");

        let req_part = Request::builder()
            .header("authorization", format!("Bearer {partial_tok}"))
            .body(())
            .unwrap();
        let (mut parts_part, ()) = req_part.into_parts();
        let res_part = AuthUser::from_request_parts(&mut parts_part, &()).await;
        assert!(res_part.is_err());
        assert_eq!(res_part.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let req_empty = Request::builder().body(()).unwrap();
        let (mut parts_empty, ()) = req_empty.into_parts();
        let res_empty = AuthUser::from_request_parts(&mut parts_empty, &()).await;
        assert!(res_empty.is_err());
        assert_eq!(res_empty.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let req_admin = Request::builder()
            .header("authorization", format!("Bearer {admin_tok}"))
            .body(())
            .unwrap();
        let (mut parts_admin, ()) = req_admin.into_parts();
        let admin = AdminUser::from_request_parts(&mut parts_admin, &())
            .await
            .unwrap();
        assert_eq!(admin.0.role, "admin");

        let req_non_admin = Request::builder()
            .header("authorization", format!("Bearer {user_tok}"))
            .body(())
            .unwrap();
        let (mut parts_non_admin, ()) = req_non_admin.into_parts();
        let res_forbidden = AdminUser::from_request_parts(&mut parts_non_admin, &()).await;
        assert!(res_forbidden.is_err());
        assert_eq!(res_forbidden.unwrap_err().0, StatusCode::FORBIDDEN);

        let mut parts_m1 = parts;
        let maybe_some = MaybeAuthUser::from_request_parts(&mut parts_m1, &())
            .await
            .unwrap();
        assert!(maybe_some.0.is_some());

        let mut parts_m2 = parts_part;
        let maybe_part = MaybeAuthUser::from_request_parts(&mut parts_m2, &())
            .await
            .unwrap();
        assert!(maybe_part.0.is_none());

        let mut parts_m3 = parts_empty;
        let maybe_none = MaybeAuthUser::from_request_parts(&mut parts_m3, &())
            .await
            .unwrap();
        assert!(maybe_none.0.is_none());
    }

    #[tokio::test]
    async fn test_logout_and_subscription() {
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

        let res_logout = logout(State(state.clone())).await;
        assert_eq!(res_logout.0["ok"], true);

        let res_sub = subscription().await;
        assert_eq!(res_sub.0["isPro"], true);
    }

    #[tokio::test]
    async fn test_login_and_verify_flows() {
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
        let username = format!("auth_test_{}", user_id.simple());
        let password = "TestPassword123!";
        let pass_hash = hash_password(password).unwrap();

        sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role, created_at, updated_at) VALUES ($1, $2, $3, 'user', NOW(), NOW())"#,
        )
        .bind(user_id)
        .bind(&username)
        .bind(&pass_hash)
        .execute(&pool)
        .await
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("127.0.0.1"));

        let res_bad_user = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: "nonexistent_user".into(),
                password: password.into(),
                totp: None,
            }),
        )
        .await;
        assert!(res_bad_user.is_err());
        assert_eq!(res_bad_user.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let res_bad_pass = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: username.clone(),
                password: "WrongPassword!".into(),
                totp: None,
            }),
        )
        .await;
        assert!(res_bad_pass.is_err());
        assert_eq!(res_bad_pass.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let res_ok = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: username.clone(),
                password: password.into(),
                totp: None,
            }),
        )
        .await
        .unwrap();
        let token = res_ok.0["token"].as_str().unwrap().to_string();
        assert!(!token.is_empty());
        assert_eq!(res_ok.0["user"]["username"], username);

        let mut verify_headers = HeaderMap::new();
        verify_headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        let res_verify = verify(verify_headers.clone(), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_verify.0["username"], username);

        let res_no_tok = verify(HeaderMap::new(), State(state.clone())).await;
        assert!(res_no_tok.is_err());
        assert_eq!(res_no_tok.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let mut bad_tok_headers = HeaderMap::new();
        bad_tok_headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer invalid.token.string"),
        );
        let res_bad_tok = verify(bad_tok_headers, State(state.clone())).await;
        assert!(res_bad_tok.is_err());
        assert_eq!(res_bad_tok.unwrap_err().0, StatusCode::UNAUTHORIZED);

        let secret = totp_rs::Secret::generate();
        let secret_str = secret.to_base32();
        sqlx::query(
            r#"INSERT INTO "two_factor_auth" (id, user_id, secret, is_enabled, created_at, updated_at) VALUES (gen_random_uuid(), $1, $2, true, NOW(), NOW())"#,
        )
        .bind(user_id)
        .bind(&secret_str)
        .execute(&pool)
        .await
        .unwrap();

        let res_2fa_req = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: username.clone(),
                password: password.into(),
                totp: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(res_2fa_req.0["twoFactorRequired"], true);
        assert!(res_2fa_req.0["token"].is_string());

        let totp = crate::two_factor::create_totp(&secret_str, &username).unwrap();
        let valid_code = totp.generate_current().to_string();
        let res_2fa_ok = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: username.clone(),
                password: password.into(),
                totp: Some(valid_code),
            }),
        )
        .await
        .unwrap();
        assert_eq!(res_2fa_ok.0["user"]["username"], username);

        let res_2fa_wrong = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: username.clone(),
                password: password.into(),
                totp: Some("000000".into()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(res_2fa_wrong.0["twoFactorRequired"], true);

        let _ = sqlx::query(r#"UPDATE "two_factor_auth" SET secret = '???' WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let res_2fa_inv_sec = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: username.clone(),
                password: password.into(),
                totp: Some("123456".into()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(res_2fa_inv_sec.0["twoFactorRequired"], true);

        let _ =
            sqlx::query(r#"UPDATE "two_factor_auth" SET is_enabled = false WHERE user_id = $1"#)
                .bind(user_id)
                .execute(&pool)
                .await;
        let res_2fa_dis_sec = login(
            headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: username.clone(),
                password: password.into(),
                totp: Some("123456".into()),
            }),
        )
        .await
        .unwrap();
        assert!(res_2fa_dis_sec.0["token"].is_string());

        assert!(
            sso_login(HeaderMap::new(), State(state.clone()), None)
                .await
                .is_err()
        );
        assert!(
            sso_login(
                HeaderMap::new(),
                State(state.clone()),
                Some(Json(SsoLoginPayload::default()))
            )
            .await
            .is_err()
        );

        assert!(
            sso_callback(Query(SsoCallbackParams {
                code: Some("auth-code".into()),
                state: None
            }))
            .await
            .is_ok()
        );
        assert!(
            sso_callback(Query(SsoCallbackParams {
                code: None,
                state: None
            }))
            .await
            .is_err()
        );

        let sso_payload = SsoLoginPayload {
            token: Some("eyJhbGciOiJIUzI1NiJ9.eyJwcmVmZXJyZWRfdXNlcm5hbWUiOiJzc29fdXNlciIsImVtYWlsIjoic3NvQGV4YW1wbGUuY29tIn0.sig".into()),
            id_token: None,
            provider: Some("oidc_provider".into()),
            ..Default::default()
        };
        let res_sso_first = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(sso_payload.clone())),
        )
        .await;
        assert!(res_sso_first.is_ok());
        let sso_uid =
            Uuid::parse_str(res_sso_first.unwrap().0["user"]["id"].as_str().unwrap()).unwrap();

        let res_sso_again = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(sso_payload.clone())),
        )
        .await;
        assert!(res_sso_again.is_ok());

        let _ = sqlx::query(r#"UPDATE "user" SET deleted_at = NOW() WHERE user_id = $1"#)
            .bind(sso_uid)
            .execute(&pool)
            .await;
        let res_sso_orphan = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(sso_payload.clone())),
        )
        .await;
        assert!(res_sso_orphan.is_ok());
        assert_eq!(res_sso_orphan.unwrap().0["user"]["role"], "view-only");
        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(sso_uid)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(sso_uid)
            .execute(&pool)
            .await;

        let pre_existing_sso_uid = Uuid::now_v7();
        let pre_existing_sso_uname = format!("sso_pre_{}", pre_existing_sso_uid.simple());
        let _ = sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role, created_at, updated_at) VALUES ($1, $2, 'hash', 'user', NOW(), NOW())"#,
        )
        .bind(pre_existing_sso_uid)
        .bind(&pre_existing_sso_uname)
        .execute(&pool)
        .await;

        let sso_jwt = format!(
            "eyJhbGciOiJIUzI1NiJ9.{}.sig",
            base64::prelude::BASE64_STANDARD_NO_PAD
                .encode(json!({ "preferred_username": pre_existing_sso_uname }).to_string())
        );
        let sso_link_payload = SsoLoginPayload {
            token: Some(sso_jwt),
            provider: Some("custom_sso_provider".into()),
            ..Default::default()
        };
        let res_sso_link = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(sso_link_payload)),
        )
        .await;
        assert!(res_sso_link.is_ok());
        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(pre_existing_sso_uid)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(pre_existing_sso_uid)
            .execute(&pool)
            .await;

        let sso_no_provider = SsoLoginPayload {
            token: Some(
                "eyJhbGciOiJIUzI1NiJ9.eyJwcmVmZXJyZWRfdXNlcm5hbWUiOiJzc29fcHJvdmlkZXJfbGVzcyJ9.sig"
                    .into(),
            ),
            provider: None,
            ..Default::default()
        };
        let res_sso_noprov = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(sso_no_provider.clone())),
        )
        .await;
        assert!(res_sso_noprov.is_ok());
        let noprov_uid =
            Uuid::parse_str(res_sso_noprov.unwrap().0["user"]["id"].as_str().unwrap()).unwrap();
        assert!(
            sso_login(
                HeaderMap::new(),
                State(state.clone()),
                Some(Json(sso_no_provider))
            )
            .await
            .is_ok()
        );
        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(noprov_uid)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(noprov_uid)
            .execute(&pool)
            .await;

        let sso_padded = format!(
            "eyJhbGciOiJIUzI1NiJ9.{}.sig",
            base64::prelude::BASE64_STANDARD
                .encode(json!({ "email": "sso_email_only@example.com" }).to_string())
        );
        let res_sso_padded = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(SsoLoginPayload {
                token: Some(sso_padded),
                ..Default::default()
            })),
        )
        .await;
        assert!(res_sso_padded.is_ok());
        let padded_uid =
            Uuid::parse_str(res_sso_padded.unwrap().0["user"]["id"].as_str().unwrap()).unwrap();
        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(padded_uid)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(padded_uid)
            .execute(&pool)
            .await;

        let sso_sub = format!(
            "eyJhbGciOiJIUzI1NiJ9.{}.sig",
            base64::prelude::BASE64_STANDARD.encode(json!({ "sub": "sso_sub_user" }).to_string())
        );
        let res_sso_sub = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(SsoLoginPayload {
                id_token: Some(sso_sub),
                ..Default::default()
            })),
        )
        .await;
        assert!(res_sso_sub.is_ok());
        let sub_uid =
            Uuid::parse_str(res_sso_sub.unwrap().0["user"]["id"].as_str().unwrap()).unwrap();
        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(sub_uid)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(sub_uid)
            .execute(&pool)
            .await;

        assert!(
            sso_login(
                HeaderMap::new(),
                State(state.clone()),
                Some(Json(SsoLoginPayload {
                    token: Some("no_dots_token".into()),
                    ..Default::default()
                }))
            )
            .await
            .is_err()
        );
        assert!(
            sso_login(
                HeaderMap::new(),
                State(state.clone()),
                Some(Json(SsoLoginPayload {
                    token: Some("header.".into()),
                    ..Default::default()
                }))
            )
            .await
            .is_err()
        );
        assert!(
            sso_login(
                HeaderMap::new(),
                State(state.clone()),
                Some(Json(SsoLoginPayload {
                    token: Some("header.!!!invalid_base64!!!.sig".into()),
                    ..Default::default()
                }))
            )
            .await
            .is_err()
        );
        assert!(
            sso_login(
                HeaderMap::new(),
                State(state.clone()),
                Some(Json(SsoLoginPayload {
                    token: Some("header.bm90X2pzb24.sig".into()),
                    ..Default::default()
                }))
            )
            .await
            .is_err()
        );
        assert!(
            sso_login(
                HeaderMap::new(),
                State(state.clone()),
                Some(Json(SsoLoginPayload {
                    token: Some("header.e30.sig".into()),
                    ..Default::default()
                }))
            )
            .await
            .is_err()
        );

        let ghost_uid = Uuid::now_v7();
        let ghost_sso_sub = format!("ghost_user_{}", ghost_uid.simple());
        let _ = sqlx::query(
            r#"INSERT INTO "user_external_auth" (id, user_id, provider, subject, created_at) VALUES ($1, $2, 'ghost_prov', $3, NOW())"#,
        )
        .bind(Uuid::now_v7())
        .bind(ghost_uid)
        .bind(&ghost_sso_sub)
        .execute(&pool)
        .await;

        let ghost_jwt = format!(
            "eyJhbGciOiJIUzI1NiJ9.{}.sig",
            base64::prelude::BASE64_STANDARD.encode(json!({ "sub": ghost_sso_sub }).to_string())
        );
        let res_ghost = sso_login(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(SsoLoginPayload {
                token: Some(ghost_jwt),
                provider: Some("ghost_prov".into()),
                ..Default::default()
            })),
        )
        .await;
        assert!(res_ghost.is_ok());
        let ghost_resp = res_ghost.unwrap().0;
        assert_eq!(ghost_resp["user"]["role"], "view-only");
        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(ghost_uid)
            .execute(&pool)
            .await;

        let mut custom_hdr_map = HeaderMap::new();
        custom_hdr_map.insert("x-remote-user", HeaderValue::from_static("hdr_user"));
        let res_sso_hdr = sso_login(custom_hdr_map, State(state.clone()), None).await;
        assert!(res_sso_hdr.is_ok());
        let hdr_uid =
            Uuid::parse_str(res_sso_hdr.unwrap().0["user"]["id"].as_str().unwrap()).unwrap();
        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(hdr_uid)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(hdr_uid)
            .execute(&pool)
            .await;

        let res_sso_wrap = sso(
            HeaderMap::new(),
            State(state.clone()),
            Some(Json(sso_payload)),
        )
        .await;
        assert!(res_sso_wrap.is_ok());

        let _ = sqlx::query(r#"DELETE FROM "user_external_auth" WHERE user_id = $1"#)
            .bind(sso_uid)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(sso_uid)
            .execute(&pool)
            .await;

        let cfg_oidc = sso_config_with_lookup(&|k| match k {
            "OIDC_ISSUER_URL" => Some("https://auth.example.com".into()),
            _ => None,
        });
        assert_eq!(cfg_oidc.0["provider"], "oidc");
        assert_eq!(cfg_oidc.0["enabled"], true);

        let cfg_header = sso_config_with_lookup(&|k| match k {
            "AUTH_HEADER" => Some("x-forwarded-user".into()),
            _ => None,
        });
        assert_eq!(cfg_header.0["provider"], "header");
        assert_eq!(cfg_header.0["enabled"], true);

        let cfg_sso = sso_config_with_lookup(&|k| match k {
            "OIDC_CLIENT_ID" => Some("client123".into()),
            _ => None,
        });
        assert_eq!(cfg_sso.0["provider"], "sso");
        assert_eq!(cfg_sso.0["enabled"], true);

        let cfg_none = sso_config_with_lookup(&|_| None);
        assert_eq!(cfg_none.0["enabled"], false);
        assert_eq!(cfg_none.0["provider"], "sso");

        assert!(sso_config().await.0["enabled"].is_boolean());

        let fake_uid = Uuid::now_v7();
        let fake_tok = make_token(fake_uid, "ghost", "user");
        let mut ghost_headers = HeaderMap::new();
        ghost_headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {fake_tok}")).unwrap(),
        );
        let res_ghost = verify(ghost_headers.clone(), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_ghost.0["username"], "ghost");

        let mut spam_headers = HeaderMap::new();
        spam_headers.insert("x-forwarded-for", HeaderValue::from_static("10.99.99.99"));
        for _ in 0..10 {
            let _ = login(
                spam_headers.clone(),
                State(state.clone()),
                Json(LoginBody {
                    username: "x".into(),
                    password: "p".into(),
                    totp: None,
                }),
            )
            .await;
        }
        let res_rate = login(
            spam_headers.clone(),
            State(state.clone()),
            Json(LoginBody {
                username: "x".into(),
                password: "p".into(),
                totp: None,
            }),
        )
        .await;
        assert!(res_rate.is_err());
        assert_eq!(res_rate.unwrap_err().0, StatusCode::TOO_MANY_REQUESTS);

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
            pool: closed_pool,
            session_cache: state.session_cache.clone(),
            website_cache: state.website_cache.clone(),
            rate_limiter: state.rate_limiter.clone(),
            ingest_queue: state.ingest_queue.clone(),
            app_secret: state.app_secret.clone(),
        };

        assert!(
            login(
                headers.clone(),
                State(err_state.clone()),
                Json(LoginBody {
                    username: username.clone(),
                    password: password.into(),
                    totp: None
                })
            )
            .await
            .is_err()
        );
        assert!(
            verify(ghost_headers.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
    }

    #[test]
    fn test_encode_jwt_token_error() {
        struct FailingSerialize;
        impl serde::Serialize for FailingSerialize {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("serialization failed"))
            }
        }
        let res = encode_jwt_token(&FailingSerialize, "test-secret");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
