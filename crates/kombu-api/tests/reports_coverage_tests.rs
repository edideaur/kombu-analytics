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
async fn test_reports_crud_and_analytics_execution() {
    let app = test_app();
    let (admin_k, admin_v) = admin_auth_header();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

    let website_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO "website" (website_id, name, domain, created_at, updated_at) VALUES ($1, 'ReportsTest', 'reports.test', NOW(), NOW())"#,
    )
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let res_create_bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "name": "Bad Report" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_create_bad.status(), StatusCode::BAD_REQUEST);

    let res_create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "websiteId": website_id.to_string(),
                        "type": "funnel",
                        "name": "Funnel Report 1",
                        "description": "Conversion funnel",
                        "parameters": { "steps": ["/home", "/checkout"] }
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
    let created_rep: Value = serde_json::from_slice(&bytes).unwrap();
    let report_id = created_rep["id"].as_str().unwrap();

    let res_list = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/reports?websiteId={website_id}&type=funnel&search=Funnel"
                ))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_list.status(), StatusCode::OK);

    let res_site_reports = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/websites/{website_id}/reports"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_site_reports.status(), StatusCode::OK);

    let res_get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/reports/{report_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get.status(), StatusCode::OK);

    let res_get_404 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/reports/{}", Uuid::now_v7()))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_get_404.status(), StatusCode::NOT_FOUND);

    let res_up = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/reports/{report_id}"))
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Updated Funnel",
                        "description": "Updated description"
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
                .uri(format!("/api/reports/{report_id}"))
                .header(admin_k, &admin_v)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_del.status(), StatusCode::OK);

    let session_id = Uuid::now_v7();
    let visit_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "session" (session_id, website_id, browser, os, device, created_at)
        VALUES ($1, $2, 'Chrome', 'Linux', 'desktop', NOW())
        "#,
    )
    .bind(session_id)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO "website_event" (
            event_id, website_id, session_id, visit_id,
            url_path, referrer_domain, utm_source, gclid,
            event_type, event_name, is_bot, created_at
        ) VALUES (
            $1, $2, $3, $4,
            '/home', 'google.com', 'google', 'gclid_test',
            1, 'pageview', false, NOW()
        )
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO "website_event" (
            event_id, website_id, session_id, visit_id,
            url_path, event_type, event_name, is_bot, created_at
        ) VALUES (
            $1, $2, $3, $4,
            '/checkout', 2, 'signup', false, NOW()
        )
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO "website_event" (
            event_id, website_id, session_id, visit_id,
            url_path, event_type, event_name, is_bot, created_at
        ) VALUES (
            $1, $2, $3, $4,
            '/api/fail', 6, 'TypeError: null is not an object', false, NOW()
        )
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO "heatmap_event" (
            heatmap_event_id, website_id, session_id, visit_id,
            url_path, x, y, page_x, page_y, viewport_w, viewport_h,
            event_type, scroll_pct, created_at
        ) VALUES (
            $1, $2, $3, $4,
            '/home', 100, 200, 100, 200, 1920, 1080,
            1, 0, NOW()
        )
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    let res_ret = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/retention")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "websiteId": website_id.to_string(), "days": 30 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ret.status(), StatusCode::OK);

    let _ = sqlx::query(
        r#"INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, event_type, created_at)
           VALUES ($1, $2, $3, $3, '/cohort-test', 1, now())"#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(session_id)
    .execute(&pool)
    .await;

    let res_cohorts = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/cohorts")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "websiteId": website_id.to_string(), "days": 14 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_cohorts.status(), StatusCode::OK);

    let _ = sqlx::query(
        r#"INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, event_type, event_name, is_bot, created_at)
           VALUES ($1, $2, $3, $3, '/signup', 2, 'signup', false, now())"#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(session_id)
    .execute(&pool)
    .await;

    for model in ["all", "first_touch", "last_touch", "linear"] {
        let res_attr = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports/attribution")
                    .header(admin_k, &admin_v)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "websiteId": website_id.to_string(),
                            "model": model,
                            "targetEvent": "signup"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_attr.status(), StatusCode::OK);
    }

    let res_err = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/errors")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "websiteId": website_id.to_string() }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_err.status(), StatusCode::OK);

    let res_heat_bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/heatmap")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_heat_bad.status(), StatusCode::BAD_REQUEST);

    let res_heat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/heatmap")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "websiteId": website_id.to_string(),
                        "urlPath": "/home"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_heat.status(), StatusCode::OK);

    let empty_site_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO "website" (website_id, name, domain, created_at, updated_at) VALUES ($1, 'EmptySite', 'empty.test', NOW(), NOW())"#
    )
    .bind(empty_site_id)
    .execute(&pool)
    .await
    .unwrap();

    let res_vitals_empty = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/performance")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "websiteId": empty_site_id.to_string(), "parameters": { "metric": "lcp" } }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_vitals_empty.status(), StatusCode::OK);

    let vitals_event_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "website_event" (
            event_id, website_id, session_id, visit_id,
            url_path, event_type, event_name, is_bot,
            lcp, cls, inp, fcp, ttfb, created_at
        ) VALUES (
            $1, $2, $3, $4,
            '/home', 1, 'pageview', false,
            2.4, 0.05, 80.0, 1.1, 0.2, NOW()
        )
        "#,
    )
    .bind(vitals_event_id)
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    for metric in ["lcp", "cls", "inp", "fcp", "ttfb", "unknown"] {
        let res_vit = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports/performance")
                    .header(admin_k, &admin_v)
                    .header("content-type", "application/json")
                    .body(Body::from(json!({
                        "websiteId": website_id.to_string(),
                        "parameters": { "metric": metric }
                    }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_vit.status(), StatusCode::OK, "Failed for performance {metric}");
    }

    let eng_event_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "website_event" (
            event_id, website_id, session_id, visit_id,
            url_path, event_type, event_name, is_bot, created_at
        ) VALUES (
            $1, $2, $3, $4,
            '/home', 2, 'scroll', false, NOW()
        )
        "#,
    )
    .bind(eng_event_id)
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO "event_data" (event_data_id, website_id, website_event_id, data_key, number_value, data_type, created_at)
        VALUES ($1, $2, $3, 'scroll_depth', 85.0, 2, NOW())
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(eng_event_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO "event_data" (event_data_id, website_id, website_event_id, data_key, number_value, data_type, created_at)
        VALUES ($1, $2, $3, 'engaged_time', 45.0, 2, NOW())
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(website_id)
    .bind(eng_event_id)
    .execute(&pool)
    .await
    .unwrap();

    let res_eng = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports/engagement")
                .header(admin_k, &admin_v)
                .header("content-type", "application/json")
                .body(Body::from(json!({
                    "websiteId": website_id.to_string(),
                    "urlPath": "/home"
                }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_eng.status(), StatusCode::OK);

    for (uri, payload) in [
        ("/api/reports/journey", json!({ "websiteId": website_id.to_string(), "steps": 3 })),
        ("/api/reports/goals", json!({ "websiteId": website_id.to_string(), "goals": [{ "name": "Signup", "type": "event", "value": "signup" }] })),
        ("/api/reports/insights", json!({ "websiteId": website_id.to_string(), "fields": ["browser", "os"] })),
        ("/api/reports/revenue", json!({ "websiteId": website_id.to_string() })),
        ("/api/reports/entry-exit", json!({ "websiteId": website_id.to_string() })),
        ("/api/reports/utm", json!({ "websiteId": website_id.to_string() })),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header(admin_k, &admin_v)
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "Failed for {uri}");
    }

    let _ = sqlx::query(r#"DELETE FROM "website_event_data" WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query(r#"DELETE FROM "heatmap_event" WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query(r#"DELETE FROM "website_event" WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query(r#"DELETE FROM "session" WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
        .bind(empty_site_id)
        .execute(&pool)
        .await;
}
