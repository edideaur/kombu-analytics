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
async fn test_teams_user_membership_management() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

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

    let member_id = Uuid::now_v7();
    let member_username = format!("member_{}", member_id.simple());
    let pass_hash = kombu_api::auth::hash_password("ValidPassword123!").unwrap();

    sqlx::query(
        r#"
        INSERT INTO "user" (user_id, username, password, role, created_at, updated_at)
        VALUES ($1, $2, $3, 'user', NOW(), NOW())
        "#,
    )
    .bind(member_id)
    .bind(&member_username)
    .bind(&pass_hash)
    .execute(&pool)
    .await
    .unwrap();

    let res_add = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/teams/{team_id}/users"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "userId": member_id.to_string(),
                        "role": "team-member"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_add.status(), StatusCode::OK);

    let res_del_u = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/teams/{team_id}/users/{member_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del_u.status(), StatusCode::OK);

    let res_join_bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/teams/join")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_join_bad.status(), StatusCode::BAD_REQUEST);

    let res_join_missing = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/teams/join")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "accessCode": "nonexistent_code" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_join_missing.status(), StatusCode::NOT_FOUND);

    let access_code = created_team["accessCode"].as_str().unwrap_or("test_code");
    let res_join_ok = app
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
    assert_eq!(res_join_ok.status(), StatusCode::OK);

    let res_team_sites = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/teams/{team_id}/websites"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_team_sites.status(), StatusCode::OK);

    let res_team_boards = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/teams/{team_id}/boards"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_team_boards.status(), StatusCode::OK);

    let res_team_pixels = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/teams/{team_id}/pixels"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_team_pixels.status(), StatusCode::OK);

    let res_team_links = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/teams/{team_id}/links"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_team_links.status(), StatusCode::OK);

    let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
        .bind(member_id)
        .execute(&pool)
        .await;
}
