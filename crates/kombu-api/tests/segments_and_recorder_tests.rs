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
async fn test_segments_and_recorder_lifecycle() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let _website_id = Uuid::now_v7();

    let site_name = format!("Site {}", Uuid::now_v7().simple());
    let res_site = app
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
                        "domain": "recorder-test.com"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_site.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_site.into_body(), usize::MAX)
        .await
        .unwrap();
    let site: Value = serde_json::from_slice(&bytes).unwrap();
    let website_id = site["id"].as_str().unwrap();

    let res_create_seg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/websites/{website_id}/segments"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "type": "custom",
                        "name": "EU Visitors",
                        "parameters": { "country": "DE" }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create_seg.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_create_seg.into_body(), usize::MAX)
        .await
        .unwrap();
    let seg: Value = serde_json::from_slice(&bytes).unwrap();
    let segment_id = seg["id"].as_str().unwrap();

    let res_list_seg = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/segments"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list_seg.status(), StatusCode::OK);

    let res_del_seg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/websites/{website_id}/segments/{segment_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del_seg.status(), StatusCode::OK);

    let session_id = Uuid::now_v7();
    let visit_id = Uuid::now_v7();
    let res_record = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/record")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "websiteId": website_id,
                        "sessionId": session_id.to_string(),
                        "visitId": visit_id.to_string(),
                        "events": [
                            { "type": 1, "data": { "test": true }, "timestamp": 12345 }
                        ],
                        "eventCount": 1,
                        "startedAt": chrono::Utc::now().to_rfc3339(),
                        "endedAt": chrono::Utc::now().to_rfc3339()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_record.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_record.into_body(), usize::MAX)
        .await
        .unwrap();
    let rec: Value = serde_json::from_slice(&bytes).unwrap();
    let replay_id = rec["replayId"].as_str().unwrap();

    let res_replay = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/websites/{website_id}/replays/{replay_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_replay.status(), StatusCode::OK);

    let res_sess_replays = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/websites/{website_id}/sessions/{session_id}/replays"
                ))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_sess_replays.status(), StatusCode::OK);
}
