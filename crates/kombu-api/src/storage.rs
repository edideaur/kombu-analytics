#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[must_use]
pub fn resolve_engine() -> kombu_core::types::StorageEngine {
    kombu_core::types::StorageEngine::detect_from_env()
}

#[must_use]
pub fn clickhouse_client() -> Option<kombu_db::ClickHouseClient> {
    clickhouse_client_from(std::env::var("CLICKHOUSE_URL").ok())
}

fn clickhouse_client_from(url: Option<String>) -> Option<kombu_db::ClickHouseClient> {
    let url = url.filter(|u| !u.trim().is_empty())?;
    kombu_db::ClickHouseClient::from_url(&url).ok()
}

pub async fn website_stats(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<kombu_query::WebsiteStats, sqlx::Error> {
    let engine = resolve_engine();
    let client = clickhouse_client();
    kombu_query::get_website_stats_with_engine(
        engine,
        pool,
        client.as_ref(),
        website_id,
        start_at,
        end_at,
    )
    .await
}

pub async fn website_metrics(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    metric_type: &str,
    limit: i64,
) -> Result<Vec<kombu_query::MetricCount>, sqlx::Error> {
    let engine = resolve_engine();
    let client = clickhouse_client();
    kombu_query::get_metrics_with_engine(
        engine,
        pool,
        client.as_ref(),
        website_id,
        start_at,
        end_at,
        metric_type,
        limit,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_engine_roundtrip() {
        let engine = resolve_engine();
        assert_eq!(
            kombu_core::types::StorageEngine::parse_str(engine.as_str()),
            Some(engine)
        );
    }

    #[test]
    fn test_clickhouse_client_from_variants() {
        assert!(clickhouse_client_from(None).is_none());
        assert!(clickhouse_client_from(Some("   ".to_string())).is_none());
        assert!(clickhouse_client_from(Some("::not a url::".to_string())).is_none());
        assert!(clickhouse_client_from(Some("http://127.0.0.1:8123/db".to_string())).is_some());
        let _ = clickhouse_client();
    }
}
