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

fn user_auth_header(user_id: Uuid) -> (&'static str, String) {
    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let claims = kombu_api::auth::Claims {
        user_id,
        username: format!("user_{}", user_id.simple()),
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
async fn test_website_full_lifecycle_and_analytics() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let site_name = format!("Site {}", Uuid::now_v7().simple());
    let domain = format!("{}.com", Uuid::now_v7().simple());

    let res_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/websites")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": site_name,
                        "domain": domain
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_create.into_body(), usize::MAX).await.unwrap();
    let site: Value = serde_json::from_slice(&bytes).unwrap();
    let website_id = site["id"].as_str().unwrap();

    let res_list_admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/websites")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list_admin.status(), StatusCode::OK);

    let random_user_id = Uuid::now_v7();
    let (user_k, user_v) = user_auth_header(random_user_id);
    let res_list_user = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/websites")
                .header(user_k, &user_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list_user.status(), StatusCode::OK);

    let res_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get.status(), StatusCode::OK);

    let res_update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": format!("{site_name} Updated")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_update.status(), StatusCode::OK);

    let analytics_endpoints = [
        format!("/api/websites/{website_id}/stats"),
        format!("/api/websites/{website_id}/active"),
        format!("/api/websites/{website_id}/daterange"),
        format!("/api/websites/{website_id}/metrics"),
        format!("/api/websites/{website_id}/pageviews"),
        format!("/api/websites/{website_id}/values"),
        format!("/api/websites/{website_id}/entry-exit"),
        format!("/api/websites/{website_id}/entry-pages"),
        format!("/api/websites/{website_id}/exit-pages"),
        format!("/api/websites/{website_id}/engagement"),
        format!("/api/websites/{website_id}/scroll"),
    ];

    for endpoint in analytics_endpoints {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&endpoint)
                    .header(admin_k, &admin_v)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "Failed for endpoint {endpoint}");
    }

    let res_reset = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}/reset"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_reset.status(), StatusCode::OK);

    let transfer_user_id = Uuid::now_v7();
    let res_transfer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}/transfer"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "userId": transfer_user_id.to_string()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_transfer.status(), StatusCode::OK);

    let unauth_user_id = Uuid::now_v7();
    let (unauth_k, unauth_v) = user_auth_header(unauth_user_id);

    let res_forbid_get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/websites/{website_id}"))
                .header(unauth_k, &unauth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forbid_get.status(), StatusCode::FORBIDDEN);

    let res_forbid_update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}"))
                .header(unauth_k, &unauth_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": "Hack" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forbid_update.status(), StatusCode::FORBIDDEN);

    let res_forbid_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/websites/{website_id}"))
                .header(unauth_k, &unauth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forbid_delete.status(), StatusCode::FORBIDDEN);

    let res_forbid_stats = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/websites/{website_id}/stats"))
                .header(unauth_k, &unauth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forbid_stats.status(), StatusCode::FORBIDDEN);

    let res_forbid_reset = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}/reset"))
                .header(unauth_k, &unauth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forbid_reset.status(), StatusCode::FORBIDDEN);

    let res_forbid_transfer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}/transfer"))
                .header(unauth_k, &unauth_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "userId": Uuid::now_v7().to_string() }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forbid_transfer.status(), StatusCode::FORBIDDEN);

    let res_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/websites/{website_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_delete.status(), StatusCode::OK);
}
