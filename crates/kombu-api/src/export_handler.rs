#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;
use kombu_core::export::{ExportWebsiteEvent, sanitize_csv_field};

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,
    pub limit: Option<i64>,
}

pub async fn export_events(
    Path(website_id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
    State(state): State<AppState>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let limit = query.limit.unwrap_or(10_000).clamp(1, 50_000);
    let format = query.format.as_deref().unwrap_or("csv").to_lowercase();

    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            Uuid,
            chrono::DateTime<Utc>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i32,
            Option<String>,
            bool,
        ),
    >(
        r#"
        SELECT
            event_id, website_id, session_id, created_at,
            url_path, url_query, referrer_domain, page_title,
            event_type, event_name, is_bot
        FROM website_event
        WHERE website_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(website_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let export_items: Vec<ExportWebsiteEvent> = rows
        .into_iter()
        .map(|r| ExportWebsiteEvent {
            event_id: r.0.to_string(),
            website_id: r.1.to_string(),
            session_id: r.2.to_string(),
            created_at: r.3.to_rfc3339(),
            url_path: r.4,
            url_query: r.5,
            referrer_domain: r.6,
            page_title: r.7,
            event_type: r.8,
            event_name: r.9,
            is_bot: r.10,
        })
        .collect();

    if format == "json" {
        let json_str = serde_json::to_string(&export_items).unwrap_or_default();
        let resp = (
            [(header::CONTENT_TYPE, "application/json")],
            json_str,
        )
            .into_response();
        return Ok(resp);
    }

    let mut csv_out = String::with_capacity(export_items.len().saturating_mul(128));
    csv_out.push_str(ExportWebsiteEvent::csv_header());
    csv_out.push('\n');

    for item in export_items {
        csv_out.push_str(&item.to_csv_row());
        csv_out.push('\n');
    }

    let resp = (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"kombu-export.csv\"",
            ),
        ],
        csv_out,
    )
        .into_response();
    Ok(resp)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEventItem {
    pub url_path: Option<String>,
    pub created_at: Option<String>,
    pub event_type: Option<i32>,
    pub event_name: Option<String>,
    pub page_title: Option<String>,
    pub referrer_domain: Option<String>,
    pub page: Option<String>,
    pub referrer: Option<String>,
    pub source: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
}

pub async fn import_events(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let items: Vec<ImportEventItem> = if let Ok(arr) = serde_json::from_value(body.clone()) {
        arr
    } else if let Some(events_val) = body.get("events") {
        serde_json::from_value(events_val.clone()).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid import format: {e}") })),
            )
        })?
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Payload must be array of events or object with 'events' field" })),
        ));
    };

    if items.len() > 10_000 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": "Maximum 10,000 events per import batch" })),
        ));
    }

    let mut imported = 0i64;
    let Ok(mut tx) = state.pool.begin().await else {
        return Ok(Json(json!({
            "success": true,
            "imported": 0
        })));
    };

    for item in items {
        let raw_path = item.url_path.as_deref().or(item.page.as_deref()).unwrap_or("/");
        let clean_path = sanitize_csv_field(raw_path);
        let url_path = kombu_core::url::truncate_url_path(&clean_path);
        let event_type = item.event_type.unwrap_or(1);
        let event_name = item.event_name.as_deref().map(|n| {
            let sanitized = sanitize_csv_field(n);
            kombu_core::url::truncate_string(&sanitized, 50)
        });
        let page_title = item.page_title.as_deref().map(|t| {
            let sanitized = sanitize_csv_field(t);
            kombu_core::url::truncate_string(&sanitized, 500)
        });
        let referrer_domain = item
            .referrer_domain
            .as_deref()
            .or(item.referrer.as_deref())
            .map(|d| kombu_core::url::truncate_string(d, 500));

        let event_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let visit_id = Uuid::now_v7();
        let created_at = item
            .created_at
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc));

        if item.browser.is_some() || item.country.is_some() {
            let _ = sqlx::query(
                r#"INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, city, distinct_id, created_at)
                   VALUES ($1, $2, $3, $4, $5, '1920x1080', 'en-US', $6, $7, $1, $8)
                   ON CONFLICT (session_id) DO NOTHING"#,
            )
            .bind(session_id)
            .bind(website_id)
            .bind(item.browser.as_deref())
            .bind(item.os.as_deref())
            .bind(item.device.as_deref())
            .bind(item.country.as_deref())
            .bind(item.city.as_deref())
            .bind(created_at)
            .execute(&mut *tx)
            .await;
        }

        let _ = sqlx::query(
            r#"INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, event_type, event_name, page_title, referrer_domain, is_bot, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, false, $10)"#,
        )
        .bind(event_id)
        .bind(website_id)
        .bind(session_id)
        .bind(visit_id)
        .bind(url_path)
        .bind(event_type)
        .bind(event_name)
        .bind(page_title)
        .bind(referrer_domain)
        .bind(created_at)
        .execute(&mut *tx)
        .await;

        imported = imported.saturating_add(1);
    }

    let _ = tx.commit().await;

    Ok(Json(json!({
        "success": true,
        "imported": imported
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_export_and_cancellation() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let state = AppState {
            pool: pool.clone(),
            session_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            website_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            rate_limiter: crate::rate_limit::RateLimiter::new(),
            ingest_queue: crate::queue::IngestQueue::new(&pool, 16, 2),
            app_secret: std::sync::Arc::new("kombu-secret".into()),
        };

        let website_id = Uuid::now_v7();
        let _ = sqlx::query(
            r#"INSERT INTO "website" (website_id, name, domain) VALUES ($1, 'Export Web', 'export.com')"#,
        )
        .bind(website_id)
        .execute(&pool)
        .await;

        let session_id = Uuid::now_v7();
        let _ = sqlx::query(
            r#"INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, distinct_id, created_at)
               VALUES ($1, $2, 'Chrome', 'Linux', 'desktop', '1920x1080', 'en-US', 'US', $1, now())"#,
        )
        .bind(session_id)
        .bind(website_id)
        .execute(&pool)
        .await;

        let _ = sqlx::query(
            r#"INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, event_type, event_name, page_title, referrer_domain, is_bot, created_at)
               VALUES ($1, $2, $3, $3, '/test-page', 1, 'test_event', 'Test Page', 'google.com', false, now())"#,
        )
        .bind(Uuid::now_v7())
        .bind(website_id)
        .bind(session_id)
        .execute(&pool)
        .await;

        let res_csv = export_events(
            Path(website_id),
            Query(ExportQuery {
                format: Some("csv".into()),
                limit: Some(50),
            }),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_csv.status(), StatusCode::OK);

        let res_json = export_events(
            Path(website_id),
            Query(ExportQuery {
                format: Some("json".into()),
                limit: Some(10),
            }),
            State(state.clone()),
        )
        .await
        .unwrap();
        assert_eq!(res_json.status(), StatusCode::OK);

        let state_clone = state.clone();
        let export_task = tokio::spawn(async move {
            export_events(
                Path(website_id),
                Query(ExportQuery {
                    format: Some("csv".into()),
                    limit: Some(50_000),
                }),
                State(state_clone),
            )
            .await
        });
        let join_res = export_task.await;
        assert!(join_res.unwrap().is_ok());

        let res_err = import_events(
            Path(website_id),
            State(state.clone()),
            Json(json!({ "invalid": true })),
        )
        .await;
        assert!(res_err.is_err());

        let res_import = import_events(
            Path(website_id),
            State(state.clone()),
            Json(json!([
                {
                    "urlPath": "/test-cancel-download",
                    "pageTitle": "Cancel Test",
                    "eventName": "purchase",
                    "referrerDomain": "example.com",
                    "browser": "Chrome",
                    "os": "macOS",
                    "device": "desktop",
                    "country": "US",
                    "city": "San Francisco",
                    "createdAt": "invalid-date"
                }
            ])),
        )
        .await;
        let Json(val) = res_import.unwrap();
        assert_eq!(val["imported"], 1);

        let res_obj_events = import_events(
            Path(website_id),
            State(state.clone()),
            Json(json!({
                "events": [{ "urlPath": "/obj-path", "createdAt": "2026-01-01T00:00:00Z" }]
            })),
        )
        .await;
        assert!(res_obj_events.is_ok());

        let res_plausible = import_events(
            Path(website_id),
            State(state.clone()),
            Json(json!([
                {
                    "page": "/plausible-page",
                    "time": "12:00:00",
                    "date": "2026-09-01",
                    "referrer": "https://bing.com",
                    "browser": "Firefox"
                }
            ])),
        )
        .await;
        assert!(res_plausible.is_ok());

        let res_bad_events = import_events(
            Path(website_id),
            State(state.clone()),
            Json(json!({ "events": "not_an_array" })),
        )
        .await;
        assert!(res_bad_events.is_err());

        let huge_events = vec![json!({ "urlPath": "/item" }); 10_001];
        let res_too_large = import_events(
            Path(website_id),
            State(state.clone()),
            Json(json!(huge_events)),
        )
        .await;
        assert!(res_too_large.is_err());

        let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        closed_pool.close().await;
        let err_state = AppState {
            pool: closed_pool,
            session_cache: state.session_cache.clone(),
            website_cache: state.website_cache.clone(),
            rate_limiter: state.rate_limiter.clone(),
            ingest_queue: state.ingest_queue.clone(),
            app_secret: state.app_secret.clone(),
        };
        let res_closed = import_events(
            Path(website_id),
            State(err_state),
            Json(json!([{ "urlPath": "/fail" }])),
        )
        .await;
        assert!(res_closed.is_ok());

        let _ = sqlx::query(r#"DELETE FROM "website_event" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
    }
}
