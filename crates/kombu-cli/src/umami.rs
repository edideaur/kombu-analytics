#![forbid(unsafe_code)]
use anyhow::Context;
use chrono::{DateTime, Utc};
use kombu_core::export::sanitize_csv_field;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UmamiEventItem {
    #[serde(alias = "url_path", alias = "page")]
    pub url_path: Option<String>,
    #[serde(alias = "created_at")]
    pub created_at: Option<String>,
    #[serde(alias = "event_type")]
    pub event_type: Option<i32>,
    #[serde(alias = "event_name", alias = "name")]
    pub event_name: Option<String>,
    #[serde(alias = "page_title", alias = "title")]
    pub page_title: Option<String>,
    #[serde(alias = "referrer_domain", alias = "referrer")]
    pub referrer_domain: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub screen: Option<String>,
    pub language: Option<String>,
}

pub fn parse_umami_json(content: &str) -> anyhow::Result<Vec<UmamiEventItem>> {
    let parsed: Value = serde_json::from_str(content).context("Invalid JSON content")?;

    if let Ok(items) = serde_json::from_value::<Vec<UmamiEventItem>>(parsed.clone()) {
        return Ok(items);
    }

    if let Some(events_val) = parsed.get("events").or_else(|| parsed.get("data")) {
        if let Ok(items) = serde_json::from_value::<Vec<UmamiEventItem>>(events_val.clone()) {
            return Ok(items);
        }
    }

    anyhow::bail!("JSON does not contain an array of events or an object with 'events'/'data'")
}

pub async fn import_umami_json(
    pool: &PgPool,
    website_id: Uuid,
    content: &str,
) -> anyhow::Result<usize> {
    let items = parse_umami_json(content)?;
    if items.is_empty() {
        return Ok(0);
    }

    let mut imported = 0usize;
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    for chunk in items.chunks(1000) {
        for item in chunk {
            let session_id = Uuid::now_v7();
            let visit_id = Uuid::now_v7();
            let event_id = Uuid::now_v7();

            let created_at: DateTime<Utc> = item
                .created_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc));

            let raw_path = item.url_path.as_deref().unwrap_or("/");
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

            let referrer_domain = item.referrer_domain.as_deref().map(|r| {
                let sanitized = sanitize_csv_field(r);
                kombu_core::url::truncate_string(&sanitized, 500)
            });

            if item.browser.is_some() || item.country.is_some() || item.os.is_some() {
                let screen = item.screen.as_deref().unwrap_or("1920x1080");
                let language = item.language.as_deref().unwrap_or("en-US");

                let _ = sqlx::query(
                    r#"INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, region, city, distinct_id, created_at)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $1, $11)
                       ON CONFLICT (session_id) DO NOTHING"#,
                )
                .bind(session_id)
                .bind(website_id)
                .bind(item.browser.as_deref())
                .bind(item.os.as_deref())
                .bind(item.device.as_deref())
                .bind(screen)
                .bind(language)
                .bind(item.country.as_deref())
                .bind(item.region.as_deref())
                .bind(item.city.as_deref())
                .bind(created_at)
                .execute(&mut *tx)
                .await;
            }

            let _ = sqlx::query(
                r#"INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, event_type, event_name, page_title, referrer_domain, is_bot, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, false, $10)"#,
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
    }

    tx.commit().await.context("Failed to commit transaction")?;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_umami_json_array() {
        let json_data = r#"[
            {
                "urlPath": "/about",
                "pageTitle": "About Us",
                "browser": "Firefox",
                "os": "Linux",
                "country": "DE",
                "createdAt": "2024-03-01T10:00:00Z"
            },
            {
                "url_path": "/contact",
                "event_type": 2,
                "event_name": "submit_form"
            }
        ]"#;

        let items = parse_umami_json(json_data).expect("parse ok");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url_path.as_deref(), Some("/about"));
        assert_eq!(items[0].browser.as_deref(), Some("Firefox"));
        assert_eq!(items[1].url_path.as_deref(), Some("/contact"));
        assert_eq!(items[1].event_name.as_deref(), Some("submit_form"));
    }

    #[test]
    fn test_parse_umami_json_object_wrapper() {
        let json_data = r#"{
            "events": [
                {
                    "page": "/pricing",
                    "referrer": "google.com"
                }
            ]
        }"#;

        let items = parse_umami_json(json_data).expect("parse ok");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url_path.as_deref(), Some("/pricing"));
        assert_eq!(items[0].referrer_domain.as_deref(), Some("google.com"));
    }

    #[test]
    fn test_parse_umami_json_invalid() {
        assert!(parse_umami_json("invalid json").is_err());
        assert!(parse_umami_json(r#"{"random": 123}"#).is_err());
    }

    #[tokio::test]
    async fn test_import_umami_json_db() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        if let Ok(pool) = PgPool::connect(&db_url).await {
            let website_id = Uuid::new_v4();
            let json_data = r#"[
                {
                    "urlPath": "/test-import",
                    "browser": "Chrome",
                    "os": "Windows",
                    "country": "US",
                    "city": "Austin"
                }
            ]"#;
            let count = import_umami_json(&pool, website_id, json_data)
                .await
                .expect("import ok");
            assert_eq!(count, 1);
        }
    }
}
