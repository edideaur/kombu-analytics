#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use uuid::Uuid;

fn test_app() -> axum::Router {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect_lazy(&url).unwrap();
    kombu_api::build_router(pool)
}

fn admin_auth_header() -> (&'static str, String) {
    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let claims = kombu_api::auth::Claims {
        user_id: uuid::Uuid::nil(),
        username: "admin".to_string(),
        role: "admin".to_string(),
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
async fn test_sessions_and_properties_and_activity() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let website_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

    let site_name = format!("Site {}", website_id.simple());
    sqlx::query(
        r#"
        INSERT INTO "website" (website_id, name, domain, created_at)
        VALUES ($1, $2, 'sess-test.com', NOW())
        "#,
    )
    .bind(website_id)
    .bind(&site_name)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, created_at)
        VALUES ($1, $2, 'Chrome', 'Linux', 'desktop', '1920x1080', 'en-US', 'US', NOW())
        "#,
    )
    .bind(session_id)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let res_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/sessions/{session_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get.status(), StatusCode::OK);

    let res_act = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/sessions/{session_id}/activity"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_act.status(), StatusCode::OK);

    let res_prop = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/sessions/{session_id}/properties"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_prop.status(), StatusCode::OK);
}
