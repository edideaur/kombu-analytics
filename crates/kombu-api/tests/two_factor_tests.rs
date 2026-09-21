#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

fn test_app() -> axum::Router {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect_lazy(&url).unwrap();
    kombu_api::build_router(pool)
}

fn user_auth_header_for(user_id: Uuid, username: &str, role: &str) -> (&'static str, String) {
    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let claims = kombu_api::auth::Claims {
        user_id,
        username: username.to_string(),
        role: role.to_string(),
        exp: (chrono::Utc::now() + chrono::Duration::days(1)).timestamp() as usize,
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();
    ("authorization", format!("Bearer {token}"))
}

#[tokio::test]
async fn test_full_two_factor_lifecycle_and_verification() {
    let app = test_app();
    let user_id = Uuid::now_v7();
    let username = format!("user2fa_{}", user_id.simple());
    let (auth_k, auth_v) = user_auth_header_for(user_id, &username, "user");

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
    let password_hash = kombu_api::auth::hash_password("ValidPassword123!").unwrap();

    sqlx::query(
        r#"
        INSERT INTO "user" (user_id, username, password, role, created_at, updated_at)
        VALUES ($1, $2, $3, 'user', NOW(), NOW())
        "#,
    )
    .bind(user_id)
    .bind(&username)
    .bind(&password_hash)
    .execute(&pool)
    .await
    .unwrap();

    let res_status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/2fa/status")
                .header(auth_k, &auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_status.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_status.into_body(), usize::MAX).await.unwrap();
    let st: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(st["isEnabled"], false);
    assert_eq!(st["isRequired"], false);

    let res_init = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/setup/initiate")
                .header(auth_k, &auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_init.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_init.into_body(), usize::MAX).await.unwrap();
    let init_json: Value = serde_json::from_slice(&bytes).unwrap();
    let _secret = init_json["secret"].as_str().unwrap();

    let res_fail_confirm = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/setup/confirm")
                .header(auth_k, &auth_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "token": "000000" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_fail_confirm.status(), StatusCode::BAD_REQUEST);

    let res_cancel = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/setup/cancel")
                .header(auth_k, &auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_cancel.status(), StatusCode::OK);

    let res_init2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/setup/initiate")
                .header(auth_k, &auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_init2.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_init2.into_body(), usize::MAX).await.unwrap();
    let init2_json: Value = serde_json::from_slice(&bytes).unwrap();
    let secret2 = init2_json["secret"].as_str().unwrap();

    let totp = kombu_api::two_factor::create_totp(secret2, &username).unwrap();
    let valid_code = totp.generate_current().to_string();

    let res_conf = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/setup/confirm")
                .header(auth_k, &auth_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "token": valid_code }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_conf.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_conf.into_body(), usize::MAX).await.unwrap();
    let conf_json: Value = serde_json::from_slice(&bytes).unwrap();
    let backup_codes = conf_json["backupCodes"].as_array().unwrap();
    assert_eq!(backup_codes.len(), 10);
    let backup_code = backup_codes[0].as_str().unwrap();

    let res_ver_backup = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/verify")
                .header(auth_k, &auth_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "backupCode": backup_code }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ver_backup.status(), StatusCode::OK);

    let res_ver_backup_again = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/verify")
                .header(auth_k, &auth_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "backupCode": backup_code }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ver_backup_again.status(), StatusCode::BAD_REQUEST);

    let res_dis = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/disable")
                .header(auth_k, &auth_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "password": "ValidPassword123!" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_dis.status(), StatusCode::OK);

    let res_st_after = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/2fa/status")
                .header(auth_k, &auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_st_after.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_st_after.into_body(), usize::MAX).await.unwrap();
    let st_after: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(st_after["isEnabled"], false);

    let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
        .bind(user_id)
        .execute(&pool)
        .await;
}
