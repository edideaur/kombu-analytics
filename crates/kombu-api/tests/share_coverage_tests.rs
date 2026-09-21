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
async fn test_share_full_crud_and_slug_routing() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let entity_id = Uuid::now_v7();

    let res_list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/share")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list.status(), StatusCode::OK);

    let slug = format!("share_{}", Uuid::now_v7().simple());
    let res_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/share")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "entityId": entity_id.to_string(),
                        "name": "Public Dashboard Share",
                        "slug": slug,
                        "shareType": 1
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_create.into_body(), usize::MAX).await.unwrap();
    let share: Value = serde_json::from_slice(&bytes).unwrap();
    let share_id = share["id"].as_str().unwrap();

    let res_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/share/id/{share_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get.status(), StatusCode::OK);

    let new_slug = format!("new_{}", Uuid::now_v7().simple());
    let res_update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/share/id/{share_id}"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Updated Share Name",
                        "slug": new_slug
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_update.status(), StatusCode::OK);

    let res_by_slug = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/share/{new_slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_by_slug.status(), StatusCode::OK);

    let res_by_alias = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/share/{slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_by_alias.status(), StatusCode::OK);

    for sub in [
        format!("/api/websites/{entity_id}/shares"),
        format!("/api/boards/{entity_id}/shares"),
        format!("/api/links/{entity_id}/shares"),
        format!("/api/pixels/{entity_id}/shares"),
    ] {
        let sub_slug = format!("sub_{}", Uuid::now_v7().simple());
        let res_sub_create = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&sub)
                    .header(admin_k, &admin_v)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Sub Share",
                            "slug": sub_slug
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_sub_create.status(), StatusCode::OK, "Failed for {sub}");
    }

    let res_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/share/id/{share_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::OK);

    let res_no_eid = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/share")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": "No Entity" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_no_eid.status(), StatusCode::BAD_REQUEST);

    let res_slash = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/share")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "entityId": entity_id.to_string(),
                        "slug": "invalid/slash"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_slash.status(), StatusCode::BAD_REQUEST);

    let res_long_slug = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/share")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "entityId": entity_id.to_string(),
                        "slug": "a".repeat(105)
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_long_slug.status(), StatusCode::BAD_REQUEST);

    let dup_slug = format!("dup_{}", Uuid::now_v7().simple());
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/share")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "entityId": entity_id.to_string(),
                        "slug": dup_slug.clone()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let res_dup = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/share")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "entityId": entity_id.to_string(),
                        "slug": dup_slug
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_dup.status(), StatusCode::BAD_REQUEST);

    let res_get_404 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/share/id/{}", Uuid::now_v7()))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get_404.status(), StatusCode::NOT_FOUND);

    let res_slug_404 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/share/nonexistent_slug_404")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_slug_404.status(), StatusCode::NOT_FOUND);

    let board_slug = format!("board_share_{}", Uuid::now_v7().simple());
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/boards/{entity_id}/shares"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "slug": board_slug.clone() }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let res_board_slug = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/share/{board_slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_board_slug.status(), StatusCode::OK);

    let link_slug = format!("link_share_{}", Uuid::now_v7().simple());
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/links/{entity_id}/shares"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "slug": link_slug.clone() }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let res_link_slug = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/share/{link_slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_link_slug.status(), StatusCode::OK);

    let pixel_slug = format!("pixel_share_{}", Uuid::now_v7().simple());
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/pixels/{entity_id}/shares"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "slug": pixel_slug.clone() }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let res_pixel_slug = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/share/{pixel_slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_pixel_slug.status(), StatusCode::OK);
}
