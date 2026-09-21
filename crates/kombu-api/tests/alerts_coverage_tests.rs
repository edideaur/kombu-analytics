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
async fn test_alerts_full_crud_and_history() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let website_id = Uuid::now_v7();

    let res_list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/alerts")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list.status(), StatusCode::OK);

    let res_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/alerts")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "websiteId": website_id.to_string(),
                        "name": "High Error Rate",
                        "alertType": "error_rate",
                        "webhookUrl": "https://example.com/webhook",
                        "channelType": "generic",
                        "threshold": 5.0,
                        "comparison": "gt",
                        "metric": "errors",
                        "windowMinutes": 15,
                        "enabled": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_create.into_body(), usize::MAX)
        .await
        .unwrap();
    let alert: Value = serde_json::from_slice(&bytes).unwrap();
    let alert_id = alert["id"].as_str().unwrap();

    let res_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/alerts/{alert_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get.status(), StatusCode::OK);

    let res_web_list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/alerts"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_web_list.status(), StatusCode::OK);

    let res_hist = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/alerts/history"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_hist.status(), StatusCode::OK);

    let res_up = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/alerts/{alert_id}"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Updated Error Rate Alert",
                        "threshold": 10.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_up.status(), StatusCode::OK);

    let res_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/alerts/{alert_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::OK);
}
