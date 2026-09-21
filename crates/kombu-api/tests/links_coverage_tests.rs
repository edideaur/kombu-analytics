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
async fn test_links_full_crud_and_redirect() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let slug = format!("lnk_{}", Uuid::now_v7().simple());

    let res_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/links")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Promo Link",
                        "url": "https://kombu.example.com/dest",
                        "slug": slug
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_create.into_body(), usize::MAX).await.unwrap();
    let link: Value = serde_json::from_slice(&bytes).unwrap();
    let link_id = link["id"].as_str().unwrap();

    let res_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/links/{link_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get.status(), StatusCode::OK);

    let res_up = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/links/{link_id}"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Updated Promo Link",
                        "url": "https://kombu.example.com/new-dest"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_up.status(), StatusCode::OK);

    let res_redir = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/q/{slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_redir.status(), StatusCode::TEMPORARY_REDIRECT);

    let res_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/links/{link_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::OK);
}
