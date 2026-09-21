#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

fn test_app() -> axum::Router {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect_lazy(&url).unwrap();
    kombu_api::build_router(pool)
}

fn user_auth_header(user_id: Uuid, username: &str) -> (&'static str, String) {
    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let claims = kombu_api::auth::Claims {
        user_id,
        username: username.to_string(),
        role: "user".to_string(),
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
async fn test_me_get_and_password_rotation() {
    let app = test_app();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

    let user_id = Uuid::now_v7();
    let username = format!("user_{}", user_id.simple());
    let (auth_k, auth_v) = user_auth_header(user_id, &username);

    let initial_pass_hash = kombu_api::auth::hash_password("OldPassword123!").unwrap();

    sqlx::query(
        r#"
        INSERT INTO "user" (user_id, username, password, role, created_at, updated_at)
        VALUES ($1, $2, $3, 'user', NOW(), NOW())
        "#,
    )
    .bind(user_id)
    .bind(&username)
    .bind(&initial_pass_hash)
    .execute(&pool)
    .await
    .unwrap();

    let res_me = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header(auth_k, &auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_me.status(), StatusCode::OK);

    let res_wrong_pass = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/me/password")
                .header(auth_k, &auth_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "currentPassword": "WrongPassword123!",
                        "newPassword": "NewPassword456!"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_wrong_pass.status(), StatusCode::BAD_REQUEST);

    let res_rot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/me/password")
                .header(auth_k, &auth_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "currentPassword": "OldPassword123!",
                        "newPassword": "NewPassword456!"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_rot.status(), StatusCode::OK);

    let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
        .bind(user_id)
        .execute(&pool)
        .await;
}
