#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn test_community_features_integration() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

    let app = kombu_api::build_router(pool.clone());

    let pixel_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/p/non-existent-pixel")
                .header("user-agent", "Mozilla/5.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let cache_control = pixel_res
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        cache_control.contains("no-cache"),
        "Pixel cache-control must include no-cache"
    );
    assert!(
        cache_control.contains("no-store"),
        "Pixel cache-control must include no-store"
    );

    let og_link_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/q/non-existent-link")
                .header(
                    "user-agent",
                    "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(og_link_res.status(), StatusCode::NOT_FOUND);

    let website_id = uuid::Uuid::parse_str("01a0aa65-c671-72cc-b248-2884c0b76273").unwrap();
    let end_at = chrono::Utc::now();
    let start_at = end_at - chrono::Duration::days(30);

    let host_metrics = kombu_query::get_metrics(&pool, website_id, start_at, end_at, "host", 10).await;
    assert!(host_metrics.is_ok(), "host metric query should succeed");

    let hostname_metrics =
        kombu_query::get_metrics(&pool, website_id, start_at, end_at, "hostname", 10).await;
    assert!(hostname_metrics.is_ok(), "hostname metric query should succeed");

    let expanded_host =
        kombu_query::get_expanded_metrics(&pool, website_id, start_at, end_at, "hostname", 10, 0)
            .await;
    assert!(expanded_host.is_ok(), "expanded hostname query should succeed");

    let overview_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/dashboard/overview?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(overview_res.status(), StatusCode::OK);

    let heartbeat_get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/heartbeat")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(heartbeat_get.status(), StatusCode::OK);

    let heartbeat_post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/heartbeat")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "website": website_id.to_string(),
                        "url": "/dashboard",
                        "hostname": "analytics.example.com"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(heartbeat_post.status(), StatusCode::OK);

    let sso_config_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/auth/sso")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sso_config_res.status(), StatusCode::OK);
}
