#![forbid(unsafe_code)]
use anyhow::Context;
use chrono::{DateTime, Utc};
use kombu_core::export::sanitize_csv_field;
use uuid::Uuid;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlausibleCsvRow {
    pub date: Option<String>,
    pub time: Option<String>,
    pub page: String,
    pub entry_page: Option<String>,
    pub exit_page: Option<String>,
    pub bounce: Option<bool>,
    pub duration: Option<i64>,
    pub referrer: Option<String>,
    pub source: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub event_name: Option<String>,
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                current.push('"');
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if c == ',' && !in_quotes {
            fields.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(c);
        }
    }
    fields.push(current.trim().to_string());
    fields
}

pub fn parse_plausible_csv(content: &str) -> anyhow::Result<Vec<PlausibleCsvRow>> {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let header_line = lines.next().context("CSV file is empty")?;
    let headers: Vec<String> = split_csv_line(header_line)
        .into_iter()
        .map(|h| h.trim_matches('"').to_lowercase())
        .collect();

    let col_date = headers.iter().position(|h| h == "date");
    let col_time = headers.iter().position(|h| h == "time");
    let col_page = headers
        .iter()
        .position(|h| {
            h == "page" || h == "page_path" || h == "url_path" || h == "path" || h == "url"
        })
        .context("Missing 'page' or 'url' column in Plausible CSV header")?;
    let col_entry = headers
        .iter()
        .position(|h| h == "entry_page" || h == "entry");
    let col_exit = headers.iter().position(|h| h == "exit_page" || h == "exit");
    let col_bounce = headers.iter().position(|h| h == "bounce" || h == "bounced");
    let col_duration = headers
        .iter()
        .position(|h| h == "visit_duration" || h == "duration");
    let col_referrer = headers
        .iter()
        .position(|h| h == "referrer" || h == "referrer_domain");
    let col_source = headers
        .iter()
        .position(|h| h == "source" || h == "utm_source");
    let col_country = headers.iter().position(|h| h == "country");
    let col_region = headers.iter().position(|h| h == "region");
    let col_city = headers.iter().position(|h| h == "city");
    let col_device = headers.iter().position(|h| h == "device");
    let col_browser = headers.iter().position(|h| h == "browser");
    let col_os = headers
        .iter()
        .position(|h| h == "os" || h == "operating_system");
    let col_event = headers
        .iter()
        .position(|h| h == "event_name" || h == "name" || h == "goal");

    let mut rows = Vec::new();
    for line in lines {
        let cols = split_csv_line(line);
        if cols.len() <= col_page {
            continue;
        }

        let get_opt = |idx: Option<usize>| -> Option<String> {
            idx.and_then(|i| cols.get(i)).and_then(|val| {
                let cleaned = val.trim_matches('"').trim();
                if cleaned.is_empty() || cleaned == "null" {
                    None
                } else {
                    Some(cleaned.to_string())
                }
            })
        };

        let raw_page = cols[col_page].trim_matches('"').trim();
        let page = if raw_page.is_empty() {
            "/".to_string()
        } else if raw_page.starts_with('/') {
            raw_page.to_string()
        } else {
            format!("/{raw_page}")
        };

        let bounce = get_opt(col_bounce).map(|v| v == "true" || v == "1" || v == "yes");
        let duration = get_opt(col_duration).and_then(|v| v.parse::<i64>().ok());

        rows.push(PlausibleCsvRow {
            date: get_opt(col_date),
            time: get_opt(col_time),
            page,
            entry_page: get_opt(col_entry),
            exit_page: get_opt(col_exit),
            bounce,
            duration,
            referrer: get_opt(col_referrer),
            source: get_opt(col_source),
            country: get_opt(col_country),
            region: get_opt(col_region),
            city: get_opt(col_city),
            device: get_opt(col_device),
            browser: get_opt(col_browser),
            os: get_opt(col_os),
            event_name: get_opt(col_event),
        });
    }

    Ok(rows)
}

pub async fn import_plausible_csv(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    content: &str,
) -> anyhow::Result<usize> {
    let rows = parse_plausible_csv(content)?;
    if rows.is_empty() {
        return Ok(0);
    }

    let mut imported = 0usize;
    let mut tx = pool.begin().await?;

    for chunk in rows.chunks(1000) {
        for row in chunk {
            let session_id = Uuid::now_v7();
            let visit_id = Uuid::now_v7();
            let event_id = Uuid::now_v7();

            let created_at: DateTime<Utc> = match (&row.date, &row.time) {
                (Some(d), Some(t)) => {
                    let ts_str = format!("{d}T{t}Z");
                    DateTime::parse_from_rfc3339(&ts_str)
                        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
                }
                (Some(d), None) => {
                    let ts_str = format!("{d}T00:00:00Z");
                    DateTime::parse_from_rfc3339(&ts_str)
                        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
                }
                _ => Utc::now(),
            };

            let clean_path = sanitize_csv_field(&row.page);
            let url_path = kombu_core::url::truncate_url_path(&clean_path);

            let event_name = row.event_name.as_deref().map(|n| {
                let sanitized = sanitize_csv_field(n);
                kombu_core::url::truncate_string(&sanitized, 50)
            });

            let event_type = if event_name.is_some() { 2 } else { 1 };

            let referrer_domain = row.referrer.as_deref().map(|r| {
                let s = sanitize_csv_field(r);
                kombu_core::url::truncate_string(&s, 500)
            });

            let _ = sqlx::query(
                r#"INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, region, city, distinct_id, created_at)
                   VALUES ($1, $2, $3, $4, $5, '1920x1080', 'en-US', $6, $7, $8, $1, $9)
                   ON CONFLICT (session_id) DO NOTHING"#,
            )
            .bind(session_id)
            .bind(website_id)
            .bind(row.browser.as_deref())
            .bind(row.os.as_deref())
            .bind(row.device.as_deref())
            .bind(row.country.as_deref())
            .bind(row.region.as_deref())
            .bind(row.city.as_deref())
            .bind(created_at)
            .execute(&mut *tx)
            .await;

            let _ = sqlx::query(
                r#"INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, event_type, event_name, page_title, referrer_domain, is_bot, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $5, $8, false, $9)"#,
            )
            .bind(event_id)
            .bind(website_id)
            .bind(session_id)
            .bind(visit_id)
            .bind(url_path)
            .bind(event_type)
            .bind(event_name)
            .bind(referrer_domain)
            .bind(created_at)
            .execute(&mut *tx)
            .await;

            imported = imported.saturating_add(1);
        }
    }

    let _ = tx.commit().await;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_csv_line_quotes_and_commas() {
        let line = r#"2026-09-01,12:00:00,"/pricing?plan=""pro"",enterprise","Google",true,42"#;
        let parts = split_csv_line(line);
        assert_eq!(parts.len(), 6);
        assert_eq!(parts[0], "2026-09-01");
        assert_eq!(parts[1], "12:00:00");
        assert_eq!(parts[2], r#"/pricing?plan="pro",enterprise"#);
        assert_eq!(parts[3], "Google");
        assert_eq!(parts[4], "true");
        assert_eq!(parts[5], "42");
    }

    #[test]
    fn test_parse_plausible_csv_full() {
        let csv = r#"date,time,page,entry_page,exit_page,bounce,visit_duration,referrer,source,country,region,city,device,browser,os,event_name
2026-09-01,10:15:30,/blog/post-1,/blog/post-1,/blog/post-1,true,15,https://news.ycombinator.com,Hacker News,US,CA,San Francisco,Desktop,Chrome,macOS,
2026-09-01,11:00:00,/signup,,,false,120,google.com,Google,GB,ENG,London,Mobile,Safari,iOS,signup_click
2026-09-02,,/date-only,,,,,,ref.com,,,,,,
,,/no-date,,,,,,,,,,,,
"#;
        let rows = parse_plausible_csv(csv).unwrap();
        assert_eq!(rows.len(), 4);

        let r1 = &rows[0];
        assert_eq!(r1.date.as_deref(), Some("2026-09-01"));
        assert_eq!(r1.time.as_deref(), Some("10:15:30"));
        assert_eq!(r1.page, "/blog/post-1");
        assert_eq!(r1.bounce, Some(true));
        assert_eq!(r1.duration, Some(15));
        assert_eq!(r1.country.as_deref(), Some("US"));
        assert_eq!(r1.event_name, None);

        let r2 = &rows[1];
        assert_eq!(r2.page, "/signup");
        assert_eq!(r2.event_name.as_deref(), Some("signup_click"));
        assert_eq!(r2.device.as_deref(), Some("Mobile"));
    }

    #[test]
    fn test_parse_plausible_csv_errors_and_edge_cases() {
        assert!(parse_plausible_csv("").is_err());

        assert!(parse_plausible_csv("date,time,country\n2026-01-01,12:00:00,US").is_err());

        let csv = "date,page\n2026-01-01,pricing\n\nshort_line_no_comma\n";
        let rows = parse_plausible_csv(csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].page, "/pricing");

        let csv2 = "page\n\"\"\n";
        let rows2 = parse_plausible_csv(csv2).unwrap();
        assert_eq!(rows2.len(), 1);
        assert_eq!(rows2[0].page, "/");
    }

    #[tokio::test]
    async fn test_import_plausible_csv_db() {
        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();
        let website_id = Uuid::now_v7();

        sqlx::query(r#"INSERT INTO "website" (website_id, name, domain, created_at) VALUES ($1, 'Plausible Test', 'plausible.test', now())"#)
            .bind(website_id)
            .execute(&pool)
            .await
            .unwrap();

        let csv = r#"date,time,page,country,referrer,event_name
2026-09-01,12:30:00,/home,US,,
2026-09-01,,/checkout,FR,https://google.com,purchase
invalid_date,invalid_time,/bad-date,,,,
2026-09-01,invalid_time,/bad-time,,,,
invalid_date_only,,/bad-date-only,,,,
,,/fallback,,,,
"#;
        let count = import_plausible_csv(&pool, website_id, csv).await.unwrap();
        assert_eq!(count, 6);

        let empty_count = import_plausible_csv(&pool, website_id, "page\n")
            .await
            .unwrap();
        assert_eq!(empty_count, 0);

        assert!(
            import_plausible_csv(&pool, website_id, "invalid_no_page_header\nfoo\n")
                .await
                .is_err()
        );

        let closed_pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://kombu:kombu@localhost:5432/kombu")
            .unwrap();
        closed_pool.close().await;
        assert!(
            import_plausible_csv(&closed_pool, website_id, "page\n/home\n")
                .await
                .is_err()
        );

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
    }
}
