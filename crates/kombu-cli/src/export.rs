#![forbid(unsafe_code)]
use anyhow::Context;
use chrono::{DateTime, Utc};
use kombu_core::export::ExportWebsiteEvent;
use sqlx::PgPool;
use std::path::Path;
use uuid::Uuid;

pub async fn run_export(
    pool: &PgPool,
    website_id: Uuid,
    format: &str,
    output: Option<&Path>,
    start_at: Option<DateTime<Utc>>,
    end_at: Option<DateTime<Utc>>,
    limit: Option<i64>,
) -> anyhow::Result<usize> {
    let limit = limit.unwrap_or(100_000).clamp(1, 1_000_000);
    let end_at = end_at.unwrap_or_else(Utc::now);
    let start_at = start_at.unwrap_or_else(|| end_at - chrono::Duration::days(365));

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
        WHERE website_id = $1 AND created_at >= $2 AND created_at <= $3
        ORDER BY created_at ASC
        LIMIT $4
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to query website events for export")?;

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

    let count = export_items.len();

    let serialized = if format.eq_ignore_ascii_case("json") {
        serde_json::to_string_pretty(&export_items).context("Failed to serialize events to JSON")?
    } else {
        let mut csv = String::with_capacity(count.saturating_mul(128));
        csv.push_str(ExportWebsiteEvent::csv_header());
        csv.push('\n');
        for item in &export_items {
            csv.push_str(&item.to_csv_row());
            csv.push('\n');
        }
        csv
    };

    if let Some(path) = output {
        std::fs::write(path, serialized)
            .with_context(|| format!("Failed to write export to {}", path.display()))?;
        println!("Exported {count} events to {}", path.display());
    } else {
        print!("{serialized}");
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_export_execution() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        if let Ok(pool) = PgPool::connect(&db_url).await {
            let tmp_out = std::env::temp_dir().join(format!("kombu_test_export_{}.csv", Uuid::now_v7()));
            let website_id = Uuid::new_v4();
            let count = run_export(&pool, website_id, "csv", Some(&tmp_out), None, None, Some(10))
                .await
                .expect("export run ok");
            assert_eq!(count, 0);
            let _ = std::fs::remove_file(tmp_out);

            let json_out = std::env::temp_dir().join(format!("kombu_test_export_{}.json", Uuid::now_v7()));
            let count2 = run_export(&pool, website_id, "json", Some(&json_out), None, None, Some(10))
                .await
                .expect("export json run ok");
            assert_eq!(count2, 0);
            let _ = std::fs::remove_file(json_out);
        }
    }
}
