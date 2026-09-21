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
async fn test_admin_and_users_and_me_full_flow() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();

    for path in [
        "/api/admin/users",
        "/api/admin/teams",
        "/api/admin/websites",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(admin_k, &admin_v)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "Failed for {path}");
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(val.get("data").is_some());
    }

    let res_users = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_users.status(), StatusCode::OK);

    let new_user_name = format!("user_{}", Uuid::now_v7().simple());
    let res_create_u = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/users")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "username": new_user_name,
                        "password": "ValidPassword123!",
                        "role": "user"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create_u.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_create_u.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_user: Value = serde_json::from_slice(&bytes).unwrap();
    let created_id = created_user["id"].as_str().unwrap();

    let res_get_u = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/users/{created_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get_u.status(), StatusCode::OK);

    let res_up_u = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/users/{created_id}"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "role": "admin" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_up_u.status(), StatusCode::OK);

    let res_u_web = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/users/{created_id}/websites"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_u_web.status(), StatusCode::OK);

    let res_u_2fa = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/admin/users/{created_id}/2fa"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_u_2fa.status(), StatusCode::OK);

    let res_upd_u_2fa = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/admin/users/{created_id}/2fa"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "required": true }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_upd_u_2fa.status(), StatusCode::OK);

    let res_u_teams = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/users/{created_id}/teams"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_u_teams.status(), StatusCode::OK);

    let res_del_u = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/users/{created_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del_u.status(), StatusCode::OK);

    for path in ["/api/me/websites", "/api/me/teams"] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(admin_k, &admin_v)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "Failed for {path}");
    }
}

#[tokio::test]
async fn test_teams_crud_and_members() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();

    let res_teams = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/teams")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_teams.status(), StatusCode::OK);

    let team_name = format!("team_{}", Uuid::now_v7().simple());
    let res_create_t = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/teams")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": team_name }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create_t.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_create_t.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_team: Value = serde_json::from_slice(&bytes).unwrap();
    let team_id = created_team["id"].as_str().unwrap();
    let access_code = created_team["accessCode"].as_str().unwrap();

    let res_get_t = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/teams/{team_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get_t.status(), StatusCode::OK);

    let res_up_t = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/teams/{team_id}"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "name": format!("{team_name}_upd") }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_up_t.status(), StatusCode::OK);

    let res_t_2fa = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/admin/teams/{team_id}/2fa"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_t_2fa.status(), StatusCode::OK);

    let res_upd_t_2fa = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/admin/teams/{team_id}/2fa"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "required": true }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_upd_t_2fa.status(), StatusCode::OK);

    for sub in ["users", "websites", "boards", "pixels", "links"] {
        let res_sub = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/teams/{team_id}/{sub}"))
                    .header(admin_k, &admin_v)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_sub.status(), StatusCode::OK, "Failed for team {sub}");
    }

    let res_join = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/teams/join")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "accessCode": access_code }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_join.status(), StatusCode::OK);

    let res_del_t = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/teams/{team_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del_t.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_boards_links_pixels_segments_revenue() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let _dummy_id = Uuid::now_v7();

    let res_b_list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/boards")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_list.status(), StatusCode::OK);

    let res_b_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/boards")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": "Analytics Board" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_b_create.into_body(), usize::MAX)
        .await
        .unwrap();
    let board: Value = serde_json::from_slice(&bytes).unwrap();
    let board_id = board["id"].as_str().unwrap();

    let res_b_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/boards/{board_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_get.status(), StatusCode::OK);

    let res_b_clone = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/boards/{board_id}/clone"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": "Cloned Board" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_clone.status(), StatusCode::OK);

    let res_b_del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/boards/{board_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_b_del.status(), StatusCode::OK);

    let res_links = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/links")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_links.status(), StatusCode::OK);

    let unique_slug = format!("slug_{}", Uuid::now_v7().simple());
    let res_link_c = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/links")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Campaign Link",
                        "url": "https://example.com/promo",
                        "slug": unique_slug
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_link_c.status(), StatusCode::OK);

    let res_pix = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/pixels")
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_pix.status(), StatusCode::OK);

    let pixel_slug = format!("px_{}", Uuid::now_v7().simple());
    let res_pix_c = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/pixels")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Email Pixel",
                        "slug": pixel_slug
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_pix_c.status(), StatusCode::OK);

    let res_w_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/websites")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Testing Subresources Site",
                        "domain": "testsub.example.com"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_w_create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res_w_create.into_body(), usize::MAX)
        .await
        .unwrap();
    let site: Value = serde_json::from_slice(&bytes).unwrap();
    let website_id = site["id"].as_str().unwrap();

    let sub_endpoints = [
        format!("/api/websites/{website_id}/events"),
        format!("/api/websites/{website_id}/events/series"),
        format!("/api/websites/{website_id}/events/stats"),
        format!("/api/websites/{website_id}/sessions"),
        format!("/api/websites/{website_id}/sessions/stats"),
        format!("/api/websites/{website_id}/sessions/weekly"),
        format!("/api/websites/{website_id}/event-data"),
        format!("/api/websites/{website_id}/event-data/properties"),
        format!("/api/websites/{website_id}/event-data/values"),
        format!("/api/websites/{website_id}/session-data/properties"),
        format!("/api/websites/{website_id}/session-data/values"),
        format!("/api/websites/{website_id}/session-data/stats"),
        format!("/api/websites/{website_id}/segments"),
        format!("/api/websites/{website_id}/revenue/stats"),
        format!("/api/websites/{website_id}/revenue/chart"),
        format!("/api/websites/{website_id}/revenue/metrics"),
        format!("/api/websites/{website_id}/revenue/sessions"),
        format!("/api/websites/{website_id}/recorder"),
        format!("/api/websites/{website_id}/replays"),
        format!("/api/websites/{website_id}/replays/saved"),
    ];

    for endpoint in sub_endpoints {
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
        assert_eq!(res.status(), StatusCode::OK, "Failed for {endpoint}");
    }
}
