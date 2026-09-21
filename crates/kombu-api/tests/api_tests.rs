#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

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

fn user_auth_header() -> (&'static str, String) {
    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let claims = kombu_api::auth::Claims {
        user_id: uuid::Uuid::new_v4(),
        username: "regular_user".to_string(),
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
async fn test_api_health() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(std::str::from_utf8(&bytes).unwrap(), "ok");
}

#[tokio::test]
async fn test_api_config() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["trackerScriptName"], "script.js");
    assert_eq!(val["cloudMode"], false);
    assert_eq!(val["telemetryDisabled"], true);
}

#[tokio::test]
async fn test_script_and_recorder_assets() {
    let app = test_app();
    let res_script = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/script.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_script.status(), StatusCode::OK);
    assert_eq!(
        res_script.headers().get("content-type").unwrap(),
        "application/javascript; charset=utf-8"
    );

    let res_recorder = app
        .oneshot(
            Request::builder()
                .uri("/recorder.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_recorder.status(), StatusCode::OK);
    assert_eq!(
        res_recorder.headers().get("content-type").unwrap(),
        "application/javascript; charset=utf-8"
    );
}

#[tokio::test]
async fn test_auth_verify_unauthorized_without_token() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/verify")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_send_invalid_payload_rejected() {
    let app = test_app();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/send")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"type":"event","payload":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response_formula = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/send")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"type":"event","payload":{"website":"550e8400-e29b-41d4-a716-446655440000","name":"=CMD|' /C calc'!A0"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response_formula.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_two_factor_status() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/2fa/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["isEnabled"], false);
}

#[tokio::test]
async fn test_two_factor_flow_endpoints() {
    let app = test_app();

    let res_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/setup/initiate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_unauth.status(), StatusCode::UNAUTHORIZED);

    let (auth_k, auth_v) = admin_auth_header();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/2fa/setup/initiate")
                .header(auth_k, auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status() == StatusCode::OK || res.status() == StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_heartbeat_route() {
    let app = test_app();
    let res_get = app
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
    assert_eq!(res_get.status(), StatusCode::OK);

    let res_post = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/heartbeat")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_post.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_reports_endpoints() {
    let app = test_app();
    let reports = [
        "/api/reports/funnel",
        "/api/reports/retention",
        "/api/reports/journey",
        "/api/reports/attribution",
        "/api/reports/revenue",
        "/api/reports/goal",
    ];

    for endpoint in reports {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(endpoint)
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK, "Failed for {endpoint}");
    }
}

#[tokio::test]
async fn test_reports_utm_envelope() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/utm")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"websiteId":"550e8400-e29b-41d4-a716-446655440000"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(val.get("utm_source").is_some());
    assert!(val.get("utm_medium").is_some());
    assert!(val.get("utm_campaign").is_some());
    assert!(val.get("utm_term").is_some());
    assert!(val.get("utm_content").is_some());
}

#[tokio::test]
async fn test_reports_performance_envelope() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/performance")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"websiteId":"550e8400-e29b-41d4-a716-446655440000","parameters":{"metric":"lcp"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(val.get("chart").is_some());
    assert!(val.get("summary").is_some());
    assert!(val.get("pages").is_some());
    assert!(val.get("pageTitles").is_some());
    assert!(val.get("devices").is_some());
    assert!(val.get("browsers").is_some());
}

#[tokio::test]
async fn test_reports_breakdown_endpoint() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/breakdown")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"websiteId":"550e8400-e29b-41d4-a716-446655440000","parameters":{"fields":["path"]}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_websites_charts_batch_endpoint() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/websites/charts?ids=550e8400-e29b-41d4-a716-446655440000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(val.get("data").is_some());
}

#[tokio::test]
async fn test_dashboard_endpoint() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["type"], "dashboard");
    assert!(val.get("parameters").is_some());
}

#[tokio::test]
async fn test_realtime_endpoint_structure() {
    let app = test_app();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/realtime/550e8400-e29b-41d4-a716-446655440000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(val.get("countries").is_some());
    assert!(val.get("urls").is_some());
    assert!(val.get("referrers").is_some());
    assert!(val.get("events").is_some());
    assert!(val.get("series").is_some());
    assert!(val.get("totals").is_some());
}

#[tokio::test]
async fn test_batch_and_performance_ingest_routes() {
    let app = test_app();
    let body = serde_json::json!({
        "type": "performance",
        "payload": {
            "website": "550e8400-e29b-41d4-a716-446655440000",
            "url": "/test-page?utm_source=test&gclid=123",
            "referrer": "https://example.com/ref",
            "cls": 0.05,
            "fcp": 120.5,
            "inp": 45.0,
            "lcp": 250.0,
            "ttfb": 80.0
        }
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/send")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let batch_payload = serde_json::json!([body]);

    let res_batch = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/batch")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&batch_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res_batch.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_shares_endpoints() {
    let app = test_app();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/boards/550e8400-e29b-41d4-a716-446655440000/shares")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(val.get("data").is_some());
    assert!(val.get("count").is_some());

    let res_web = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/websites/550e8400-e29b-41d4-a716-446655440000/shares")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_web.status(), StatusCode::OK);

    let res_missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/share/nonexistentslug123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res_missing.status() == StatusCode::INTERNAL_SERVER_ERROR
            || res_missing.status() == StatusCode::NOT_FOUND
    );

    let res_slash = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/boards/550e8400-e29b-41d4-a716-446655440000/shares")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Invalid Share",
                        "slug": "invalid/slug"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_slash.status(), StatusCode::BAD_REQUEST);

    let res_update_slash = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/share/id/550e8400-e29b-41d4-a716-446655440000")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Invalid Share",
                        "slug": "invalid/slug"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_update_slash.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_new_parity_endpoints() {
    let app = test_app();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/auth/subscription")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(val["isPro"], true);

    let res_telemetry = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/scripts/telemetry")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_telemetry.status(), StatusCode::OK);

    let res_links_charts = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/links/charts?ids=550e8400-e29b-41d4-a716-446655440000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_links_charts.status(), StatusCode::OK);

    let res_pixels_charts = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/pixels/charts?ids=550e8400-e29b-41d4-a716-446655440000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_pixels_charts.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_client_error_and_reporting() {
    let app = test_app();

    let payload = json!({
        "type": "error",
        "payload": {
            "website": "550e8400-e29b-41d4-a716-446655440000",
            "url": "/dashboard",
            "message": "Uncaught ReferenceError: foo is not defined",
            "stack": "ReferenceError: foo is not defined\n    at index.js:10:5"
        }
    });

    let res_send = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/send")
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_send.status(), StatusCode::BAD_REQUEST);

    let res_batch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/batch")
                .header("Content-Type", "application/json")
                .body(Body::from(json!([payload]).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_batch.status(), StatusCode::OK);

    let res_report = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/errors")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({ "websiteId": "550e8400-e29b-41d4-a716-446655440000" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_report.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_report.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(val.get("errors").is_some());
    assert!(val.get("count").is_some());
}

#[tokio::test]
async fn test_export_and_import_endpoints() {
    let app = test_app();

    let res_csv = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/websites/550e8400-e29b-41d4-a716-446655440000/export?format=csv")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_csv.status(), StatusCode::OK);
    assert_eq!(
        res_csv.headers().get("content-type").unwrap(),
        "text/csv; charset=utf-8"
    );

    let res_json = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/websites/550e8400-e29b-41d4-a716-446655440000/export?format=json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_json.status(), StatusCode::OK);
    assert_eq!(
        res_json.headers().get("content-type").unwrap(),
        "application/json"
    );

    let import_payload = json!([
        {
            "urlPath": "=HYPERLINK(\"http://evil.com\")",
            "pageTitle": "@SUM(1,2)",
            "eventType": 1
        }
    ]);
    let res_import = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/websites/550e8400-e29b-41d4-a716-446655440000/import")
                .header("Content-Type", "application/json")
                .body(Body::from(import_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_import.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_bot_traffic_not_rejected() {
    let app = test_app();

    let payload = json!({
        "type": "event",
        "payload": {
            "website": "550e8400-e29b-41d4-a716-446655440000",
            "url": "/docs",
            "referrer": "http://darodar.com",
            "userAgent": "Googlebot/2.1 (+http://www.google.com/bot.html)"
        }
    });

    let res_batch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/batch")
                .header(
                    "User-Agent",
                    "Googlebot/2.1 (+http://www.google.com/bot.html)",
                )
                .header("Content-Type", "application/json")
                .body(Body::from(json!([payload]).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_batch.status(), StatusCode::OK);

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/send")
                .header(
                    "User-Agent",
                    "Googlebot/2.1 (+http://www.google.com/bot.html)",
                )
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_pixel_slug_change_and_custom_image() {
    let app = test_app();

    let res_list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/pixels")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list.status(), StatusCode::OK);

    let res_render = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/p/nonexistent-pixel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_render.status(), StatusCode::NOT_FOUND);

    let update_payload = json!({
        "name": "Updated Pixel",
        "slug": "custom-new-slug",
        "image": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
    });

    let res_update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/pixels/550e8400-e29b-41d4-a716-446655440000")
                .header("Content-Type", "application/json")
                .body(Body::from(update_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        res_update.status() == StatusCode::NOT_FOUND
            || res_update.status() == StatusCode::BAD_REQUEST
            || res_update.status() == StatusCode::INTERNAL_SERVER_ERROR
    );

    let png_bytes = vec![
        0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 0,
    ];
    let res_img_upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/pixels/550e8400-e29b-41d4-a716-446655440000/image")
                .header("Content-Type", "image/png")
                .body(Body::from(png_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res_img_upload.status() == StatusCode::NOT_FOUND
            || res_img_upload.status() == StatusCode::INTERNAL_SERVER_ERROR
    );

    let res_img_del = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/pixels/550e8400-e29b-41d4-a716-446655440000/image")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res_img_del.status() == StatusCode::NOT_FOUND
            || res_img_del.status() == StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn test_new_reports_and_alerts_endpoints() {
    let app = test_app();

    let cohorts_payload = json!({
        "websiteId": "550e8400-e29b-41d4-a716-446655440000",
        "days": 30
    });
    let res_cohorts = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/cohorts")
                .header("Content-Type", "application/json")
                .body(Body::from(cohorts_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_cohorts.status(), StatusCode::OK);

    let attr_payload = json!({
        "websiteId": "550e8400-e29b-41d4-a716-446655440000",
        "model": "all"
    });
    let res_attr = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/attribution")
                .header("Content-Type", "application/json")
                .body(Body::from(attr_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_attr.status(), StatusCode::OK);

    let res_alerts = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/alerts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_alerts.status(), StatusCode::OK);

    let res_site_alerts = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/websites/550e8400-e29b-41d4-a716-446655440000/alerts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_site_alerts.status(), StatusCode::OK);

    let res_history = app
        .oneshot(
            Request::builder()
                .uri("/api/websites/550e8400-e29b-41d4-a716-446655440000/alerts/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_history.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_annotations_and_engagement_and_entry_exit() {
    let app = test_app();
    let website_id = "550e8400-e29b-41d4-a716-446655440000";

    let entry_exit_payload = json!({
        "websiteId": website_id,
        "limit": 10
    });
    let res_ee = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/entry-exit")
                .header("Content-Type", "application/json")
                .body(Body::from(entry_exit_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ee.status(), StatusCode::OK);
    let bytes_ee = axum::body::to_bytes(res_ee.into_body(), usize::MAX)
        .await
        .unwrap();
    let val_ee: Value = serde_json::from_slice(&bytes_ee).unwrap();
    assert!(val_ee.get("entryPages").is_some());
    assert!(val_ee.get("exitPages").is_some());

    let eng_payload = json!({
        "websiteId": website_id
    });
    let res_eng = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/engagement")
                .header("Content-Type", "application/json")
                .body(Body::from(eng_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_eng.status(), StatusCode::OK);
    let bytes_eng = axum::body::to_bytes(res_eng.into_body(), usize::MAX)
        .await
        .unwrap();
    let val_eng: Value = serde_json::from_slice(&bytes_eng).unwrap();
    assert!(val_eng.get("avgScrollDepth").is_some());
    assert!(val_eng.get("milestones").is_some());

    let res_scroll = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/scroll")
                .header("Content-Type", "application/json")
                .body(Body::from(eng_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_scroll.status(), StatusCode::OK);

    let (admin_k, admin_v) = admin_auth_header();
    let res_ep = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/entry-pages"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ep.status(), StatusCode::OK);

    let res_xp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/exit-pages"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_xp.status(), StatusCode::OK);

    let res_we_eng = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/engagement"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_we_eng.status(), StatusCode::OK);

    let res_anno_list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/annotations"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_anno_list.status(), StatusCode::OK);
    let bytes_anno = axum::body::to_bytes(res_anno_list.into_body(), usize::MAX)
        .await
        .unwrap();
    let val_anno: Value = serde_json::from_slice(&bytes_anno).unwrap();
    assert!(val_anno.get("data").is_some());

    let fake_anno_id = "01940000-0000-7000-8000-000000000001";
    let res_get_anno = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/annotations/{fake_anno_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get_anno.status(), StatusCode::NOT_FOUND);

    let res_ret = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/retention"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ret.status(), StatusCode::OK);
    let bytes_ret = axum::body::to_bytes(res_ret.into_body(), usize::MAX)
        .await
        .unwrap();
    let val_ret: Value = serde_json::from_slice(&bytes_ret).unwrap();
    assert!(val_ret.get("retentionDays").is_some());
    assert!(val_ret.get("stats").is_some());

    let purge_payload = json!({
        "days": 60,
        "urlPath": "/test-purge"
    });
    let res_purge = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}/retention/purge"))
                .header("Content-Type", "application/json")
                .body(Body::from(purge_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_purge.status(), StatusCode::OK);
    let bytes_purge = axum::body::to_bytes(res_purge.into_body(), usize::MAX)
        .await
        .unwrap();
    let val_purge: Value = serde_json::from_slice(&bytes_purge).unwrap();
    assert_eq!(val_purge["ok"], true);
    assert!(val_purge.get("purgedEvents").is_some());

    let res_hist = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/retention/history"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_hist.status(), StatusCode::OK);

    let res_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/retention/purge")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_unauth.status(), StatusCode::UNAUTHORIZED);

    let (auth_k, auth_v) = admin_auth_header();
    let res_admin_purge = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/retention/purge")
                .header(auth_k, auth_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_admin_purge.status(), StatusCode::OK);
    let bytes_admin = axum::body::to_bytes(res_admin_purge.into_body(), usize::MAX)
        .await
        .unwrap();
    let val_admin: Value = serde_json::from_slice(&bytes_admin).unwrap();
    assert_eq!(val_admin["ok"], true);
}

#[tokio::test]
async fn test_admin_and_user_rbac_enforcement() {
    let app = test_app();

    let res_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_unauth.status(), StatusCode::UNAUTHORIZED);

    let (u_k, u_v) = user_auth_header();
    let res_forbidden = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin/users")
                .header(u_k, u_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_forbidden.status(), StatusCode::FORBIDDEN);

    let res_users_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_users_unauth.status(), StatusCode::UNAUTHORIZED);

    let (u_k, u_v) = user_auth_header();
    let res_create_forbidden = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header("content-type", "application/json")
                .header(u_k, u_v)
                .body(Body::from(r#"{"username":"hacker","password":"password123"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create_forbidden.status(), StatusCode::FORBIDDEN);

    let res_web_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/websites")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"test","domain":"test.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_web_unauth.status(), StatusCode::UNAUTHORIZED);

    let res_del_unauth = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/websites/550e8400-e29b-41d4-a716-446655440000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del_unauth.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_password_hashing_and_verification() {
    let too_short = kombu_api::auth::hash_password("short");
    assert!(too_short.is_err());

    let plain = "SuperSecretPassword123!";
    let hashed = kombu_api::auth::hash_password(plain).unwrap();
    assert!(hashed.starts_with("$2"));

    assert!(kombu_api::auth::verify_password(plain, &hashed));

    assert!(!kombu_api::auth::verify_password("WrongPassword123!", &hashed));

    assert!(!kombu_api::auth::verify_password("kombu", &hashed));
    assert!(!kombu_api::auth::verify_password("umami", &hashed));
}

#[tokio::test]
async fn test_rate_limiting_enforcement() {
    let limiter = kombu_api::rate_limit::RateLimiter::new();
    let key = "test_client_ip";

    assert!(limiter.check(key, 3, 60).is_ok());
    assert!(limiter.check(key, 3, 60).is_ok());
    assert!(limiter.check(key, 3, 60).is_ok());

    let rejected = limiter.check(key, 3, 60);
    assert!(rejected.is_err());
    let (status, headers, _) = rejected.unwrap_err();
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.get("retry-after").is_some());
}
