#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ClickHouseError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Invalid ClickHouse URL: {0}")]
    InvalidUrl(String),
    #[error("ClickHouse server error (status {0}): {1}")]
    Server(u16, String),
    #[error("Deserialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickHouseConfig {
    pub endpoint: String,
    pub database: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ClickHouseConfig {
    pub fn from_url(url_str: &str) -> Result<Self, ClickHouseError> {
        let trimmed = url_str.trim();
        if trimmed.is_empty() {
            return Err(ClickHouseError::InvalidUrl("Empty URL".to_string()));
        }

        let parsed = reqwest::Url::parse(trimmed)
            .map_err(|e| ClickHouseError::InvalidUrl(e.to_string()))?;

        let host = parsed
            .host_str()
            .ok_or_else(|| ClickHouseError::InvalidUrl("Missing host".to_string()))?;

        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(ClickHouseError::InvalidUrl(
                "Scheme must be http or https".to_string(),
            ));
        }

        let port_part = match parsed.port() {
            Some(p) => format!(":{p}"),
            None => String::new(),
        };

        let endpoint = format!("{scheme}://{host}{port_part}");

        let path = parsed.path().trim_start_matches('/');
        let database = if path.is_empty() {
            "default".to_string()
        } else {
            path.to_string()
        };

        let username = if parsed.username().is_empty() {
            None
        } else {
            Some(parsed.username().to_string())
        };

        let password = parsed.password().map(ToString::to_string);

        Ok(Self {
            endpoint,
            database,
            username,
            password,
        })
    }
}

#[derive(Clone)]
pub struct ClickHouseClient {
    config: ClickHouseConfig,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickHouseEvent {
    pub website_id: Uuid,
    pub session_id: Uuid,
    pub visit_id: Uuid,
    pub event_id: Uuid,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub browser: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub screen: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub url_path: String,
    pub url_query: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_content: Option<String>,
    pub utm_term: Option<String>,
    pub referrer_path: Option<String>,
    pub referrer_query: Option<String>,
    pub referrer_domain: Option<String>,
    pub page_title: Option<String>,
    pub gclid: Option<String>,
    pub fbclid: Option<String>,
    pub msclkid: Option<String>,
    pub ttclid: Option<String>,
    pub li_fat_id: Option<String>,
    pub twclid: Option<String>,
    pub lcp: Option<f64>,
    pub inp: Option<f64>,
    pub cls: Option<f64>,
    pub fcp: Option<f64>,
    pub ttfb: Option<f64>,
    pub event_type: u32,
    pub event_name: Option<String>,
    pub tag: Option<String>,
    pub distinct_id: Option<String>,
    pub created_at: String,
}

impl ClickHouseClient {
    pub fn new(config: ClickHouseConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .ok()
            .unwrap_or_default();

        Self { config, client }
    }

    pub fn from_url(url: &str) -> Result<Self, ClickHouseError> {
        let config = ClickHouseConfig::from_url(url)?;
        Ok(Self::new(config))
    }

    pub fn config(&self) -> &ClickHouseConfig {
        &self.config
    }

    pub async fn ping(&self) -> Result<bool, ClickHouseError> {
        let ping_url = format!("{}/ping", self.config.endpoint);
        let resp = self.client.get(&ping_url).send().await?;
        Ok(resp.status().is_success())
    }

    pub async fn execute(&self, query: &str) -> Result<(), ClickHouseError> {
        let mut req = self
            .client
            .post(&self.config.endpoint)
            .query(&[("database", &self.config.database)])
            .body(query.to_string());

        if let Some(user) = &self.config.username {
            req = req.basic_auth(user, self.config.password.as_deref());
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ClickHouseError::Server(status, body));
        }

        Ok(())
    }

    pub async fn query_json<T: DeserializeOwned>(
        &self,
        query: &str,
    ) -> Result<Vec<T>, ClickHouseError> {
        let sql = if query.to_ascii_uppercase().contains("FORMAT ") {
            query.to_string()
        } else {
            format!("{query} FORMAT JSONEachRow")
        };

        let mut req = self
            .client
            .post(&self.config.endpoint)
            .query(&[("database", &self.config.database)])
            .body(sql);

        if let Some(user) = &self.config.username {
            req = req.basic_auth(user, self.config.password.as_deref());
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ClickHouseError::Server(status, body));
        }

        let text = resp.text().await?;
        let mut results = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() {
                let parsed: T = serde_json::from_str(line)?;
                results.push(parsed);
            }
        }

        Ok(results)
    }

    pub async fn insert_events(
        &self,
        events: &[ClickHouseEvent],
    ) -> Result<(), ClickHouseError> {
        if events.is_empty() {
            return Ok(());
        }

        let mut payload = String::new();
        for ev in events {
            let s = serde_json::to_string(ev).unwrap_or_default();
            payload.push_str(&s);
            payload.push('\n');
        }

        let mut req = self
            .client
            .post(&self.config.endpoint)
            .query(&[
                (
                    "query",
                    "INSERT INTO website_event FORMAT JSONEachRow",
                ),
                ("database", &self.config.database),
            ])
            .header("Content-Type", "application/json")
            .body(payload);

        if let Some(user) = &self.config.username {
            req = req.basic_auth(user, self.config.password.as_deref());
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ClickHouseError::Server(status, body));
        }

        Ok(())
    }

    pub async fn apply_schema(&self) -> Result<(), ClickHouseError> {
        let db_query = format!("CREATE DATABASE IF NOT EXISTS `{}`", self.config.database);
        let mut req = self
            .client
            .post(&self.config.endpoint)
            .body(db_query);

        if let Some(user) = &self.config.username {
            req = req.basic_auth(user, self.config.password.as_deref());
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ClickHouseError::Server(status, body));
        }

        let create_events = r#"
        CREATE TABLE IF NOT EXISTS website_event
        (
            website_id UUID,
            session_id UUID,
            visit_id UUID,
            event_id UUID,
            hostname LowCardinality(String),
            browser LowCardinality(String),
            os LowCardinality(String),
            device LowCardinality(String),
            screen LowCardinality(String),
            language LowCardinality(String),
            country LowCardinality(String),
            region LowCardinality(String),
            city String,
            url_path String,
            url_query Nullable(String),
            utm_source Nullable(String),
            utm_medium Nullable(String),
            utm_campaign Nullable(String),
            utm_content Nullable(String),
            utm_term Nullable(String),
            referrer_path Nullable(String),
            referrer_query Nullable(String),
            referrer_domain Nullable(String),
            page_title Nullable(String),
            gclid Nullable(String),
            fbclid Nullable(String),
            msclkid Nullable(String),
            ttclid Nullable(String),
            li_fat_id Nullable(String),
            twclid Nullable(String),
            lcp Nullable(Decimal(10, 1)),
            inp Nullable(Decimal(10, 1)),
            cls Nullable(Decimal(10, 4)),
            fcp Nullable(Decimal(10, 1)),
            ttfb Nullable(Decimal(10, 1)),
            event_type UInt32,
            event_name Nullable(String),
            tag Nullable(String),
            distinct_id Nullable(String),
            created_at DateTime('UTC')
        )
        ENGINE = MergeTree
        PARTITION BY toYYYYMM(created_at)
        ORDER BY (toStartOfHour(created_at), website_id, session_id, visit_id, created_at)
        PRIMARY KEY (toStartOfHour(created_at), website_id, session_id, visit_id)
        SETTINGS index_granularity = 8192;
        "#;

        self.execute(create_events).await?;

        let create_event_data = r#"
        CREATE TABLE IF NOT EXISTS event_data
        (
            website_id UUID,
            session_id UUID,
            event_id UUID,
            url_path String,
            event_name String,
            data_key String,
            string_value Nullable(String),
            number_value Nullable(Decimal(22, 4)),
            date_value Nullable(DateTime('UTC')),
            data_type UInt32,
            created_at DateTime('UTC')
        )
        ENGINE = MergeTree
        ORDER BY (website_id, event_id, data_key, created_at)
        SETTINGS index_granularity = 8192;
        "#;

        self.execute(create_event_data).await?;

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_clickhouse_config_from_url_variations() {
        let cfg = ClickHouseConfig::from_url("http://localhost:8123").unwrap();
        assert_eq!(cfg.endpoint, "http://localhost:8123");
        assert_eq!(cfg.database, "default");
        assert!(cfg.username.is_none());
        assert!(cfg.password.is_none());

        let cfg2 = ClickHouseConfig::from_url("https://myuser:secret123@clickhouse.internal:8443/analytics").unwrap();
        assert_eq!(cfg2.endpoint, "https://clickhouse.internal:8443");
        assert_eq!(cfg2.database, "analytics");
        assert_eq!(cfg2.username.as_deref(), Some("myuser"));
        assert_eq!(cfg2.password.as_deref(), Some("secret123"));

        assert!(ClickHouseConfig::from_url("").is_err());
        assert!(ClickHouseConfig::from_url("ftp://localhost").is_err());
        assert!(ClickHouseConfig::from_url("http://").is_err());
        assert!(ClickHouseConfig::from_url("not-a-valid-url").is_err());

        let cfg_no_port = ClickHouseConfig::from_url("http://localhost/mydb").unwrap();
        assert_eq!(cfg_no_port.endpoint, "http://localhost");
        assert_eq!(cfg_no_port.database, "mydb");

        assert!(ClickHouseConfig::from_url("foo:bar").is_err());
    }

    #[test]
    fn test_clickhouse_client_creation_and_config() {
        let client = ClickHouseClient::from_url("http://127.0.0.1:8123/kombu").unwrap();
        assert_eq!(client.config().database, "kombu");
        assert!(ClickHouseClient::from_url("").is_err());
    }

    #[test]
    fn test_clickhouse_event_serialization() {
        let ev = ClickHouseEvent {
            website_id: Uuid::nil(),
            session_id: Uuid::nil(),
            visit_id: Uuid::nil(),
            event_id: Uuid::nil(),
            hostname: "example.com".into(),
            browser: "Chrome".into(),
            os: "Linux".into(),
            device: "desktop".into(),
            screen: "1920x1080".into(),
            language: "en".into(),
            country: "US".into(),
            region: "CA".into(),
            city: "San Francisco".into(),
            url_path: "/home".into(),
            url_query: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            referrer_path: None,
            referrer_query: None,
            referrer_domain: None,
            page_title: Some("Home".into()),
            gclid: None,
            fbclid: None,
            msclkid: None,
            ttclid: None,
            li_fat_id: None,
            twclid: None,
            lcp: Some(1.2),
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: 1,
            event_name: None,
            tag: None,
            distinct_id: None,
            created_at: "2026-09-20 12:00:00".into(),
        };

        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("example.com"));
        assert!(json.contains("/home"));

        let err_display = format!("{}", ClickHouseError::InvalidUrl("test_err".into()));
        assert!(err_display.contains("test_err"));
        let err_server = format!("{}", ClickHouseError::Server(500, "internal".into()));
        assert!(err_server.contains("500"));
    }

    #[tokio::test]
    async fn test_clickhouse_mock_server_flow() {
        use axum::{routing::post, routing::get, Router, response::IntoResponse};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let post_count = Arc::new(AtomicUsize::new(0));
        let count_clone = post_count.clone();

        let app = Router::new()
            .route(
                "/ping",
                get(|| async { "Ok.\n" }),
            )
            .route(
                "/",
                post(move |body: String| {
                    let c = count_clone.clone();
                    async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        if body.contains("BADJSON") {
                            "this is not json\n".into_response()
                        } else if body.contains("FORMAT JSONEachRow") {
                            "{\"pageviews\":12,\"visitors\":4,\"visits\":5,\"bounces\":1,\"totaltime\":50}\n\n".into_response()
                        } else if body.contains("FAIL") {
                            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "DB fail").into_response()
                        } else {
                            "OK\n".into_response()
                        }
                    }
                }),
            );

        let (port, shutdown_tx, server_handle) = spawn_test_server(app).await;

        let client = ClickHouseClient::from_url(&format!("http://myuser:secret@127.0.0.1:{port}/test_kombu")).unwrap();

        let ping_ok = client.ping().await.unwrap();
        assert!(ping_ok);

        let schema_ok = client.apply_schema().await;
        assert!(schema_ok.is_ok());

        let ev = ClickHouseEvent {
            website_id: Uuid::nil(),
            session_id: Uuid::nil(),
            visit_id: Uuid::nil(),
            event_id: Uuid::nil(),
            hostname: "mock.local".into(),
            browser: "Chrome".into(),
            os: "Linux".into(),
            device: "desktop".into(),
            screen: "1920x1080".into(),
            language: "en".into(),
            country: "US".into(),
            region: "CA".into(),
            city: "SF".into(),
            url_path: "/".into(),
            url_query: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            referrer_path: None,
            referrer_query: None,
            referrer_domain: None,
            page_title: None,
            gclid: None,
            fbclid: None,
            msclkid: None,
            ttclid: None,
            li_fat_id: None,
            twclid: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: 1,
            event_name: None,
            tag: None,
            distinct_id: None,
            created_at: "2026-09-20 12:00:00".into(),
        };

        let insert_empty = client.insert_events(&[]).await;
        assert!(insert_empty.is_ok());

        let insert_one = client.insert_events(&[ev]).await;
        assert!(insert_one.is_ok());

        let rows: Vec<serde_json::Value> = client.query_json("SELECT 1").await.unwrap();
        assert_eq!(rows.len(), 1);

        let rows_with_format: Vec<serde_json::Value> = client.query_json("SELECT 1 FORMAT JSONEachRow").await.unwrap();
        assert_eq!(rows_with_format.len(), 1);

        let bad_rows: Result<Vec<serde_json::Value>, _> = client.query_json("SELECT BADJSON").await;
        assert!(bad_rows.is_err());

        let fail_exec = client.execute("FAIL THIS QUERY").await;
        assert!(fail_exec.is_err());

        let fail_app = Router::new()
            .route("/ping", get(|| async { (axum::http::StatusCode::BAD_REQUEST, "err") }))
            .route("/", post(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "err") }));
        let (fail_port, fail_shutdown_tx, fail_server_handle) = spawn_test_server(fail_app).await;
        let fail_client = ClickHouseClient::from_url(&format!("http://myuser:secret@127.0.0.1:{fail_port}/fail_db")).unwrap();

        assert!(!fail_client.ping().await.unwrap());
        assert!(fail_client.apply_schema().await.is_err());
        let bad_ev = ClickHouseEvent {
            website_id: Uuid::nil(),
            session_id: Uuid::nil(),
            visit_id: Uuid::nil(),
            event_id: Uuid::nil(),
            hostname: String::new(),
            browser: String::new(),
            os: String::new(),
            device: String::new(),
            screen: String::new(),
            language: String::new(),
            country: String::new(),
            region: String::new(),
            city: String::new(),
            url_path: String::new(),
            url_query: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            referrer_path: None,
            referrer_query: None,
            referrer_domain: None,
            page_title: None,
            gclid: None,
            fbclid: None,
            msclkid: None,
            ttclid: None,
            li_fat_id: None,
            twclid: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: 0,
            event_name: None,
            tag: None,
            distinct_id: None,
            created_at: String::new(),
        };
        assert!(fail_client.insert_events(&[bad_ev]).await.is_err());
        let query_err: Result<Vec<serde_json::Value>, _> = fail_client.query_json("SELECT 1").await;
        assert!(query_err.is_err());

        let no_auth = ClickHouseClient::from_url(&format!("http://127.0.0.1:{port}/test_kombu")).unwrap();
        assert!(no_auth.execute("SELECT 1").await.is_ok());
        let no_auth_rows: Vec<serde_json::Value> = no_auth.query_json("SELECT 1").await.unwrap();
        assert_eq!(no_auth_rows.len(), 1);
        assert!(no_auth.insert_events(&[]).await.is_ok());
        assert!(no_auth.apply_schema().await.is_ok());

        let _ = shutdown_tx.send(());
        let _ = server_handle.await;
        let _ = fail_shutdown_tx.send(());
        let _ = fail_server_handle.await;
    }

    async fn spawn_test_server(
        app: axum::Router,
    ) -> (
        u16,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        (port, tx, handle)
    }

    #[tokio::test]
    async fn test_clickhouse_error_paths() {
        use axum::{routing::post, Router, response::IntoResponse};
        use tokio::io::AsyncWriteExt;

        let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_port = dead_listener.local_addr().unwrap().port();
        drop(dead_listener);
        let dead = ClickHouseClient::from_url(&format!("http://127.0.0.1:{dead_port}/db")).unwrap();
        assert!(dead.ping().await.is_err());
        assert!(dead.execute("SELECT 1").await.is_err());
        let dead_rows: Result<Vec<serde_json::Value>, _> = dead.query_json("SELECT 1").await;
        assert!(dead_rows.is_err());
        let ev = ClickHouseEvent {
            website_id: Uuid::nil(),
            session_id: Uuid::nil(),
            visit_id: Uuid::nil(),
            event_id: Uuid::nil(),
            hostname: String::new(),
            browser: String::new(),
            os: String::new(),
            device: String::new(),
            screen: String::new(),
            language: String::new(),
            country: String::new(),
            region: String::new(),
            city: String::new(),
            url_path: String::new(),
            url_query: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            referrer_path: None,
            referrer_query: None,
            referrer_domain: None,
            page_title: None,
            gclid: None,
            fbclid: None,
            msclkid: None,
            ttclid: None,
            li_fat_id: None,
            twclid: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: 0,
            event_name: None,
            tag: None,
            distinct_id: None,
            created_at: String::new(),
        };
        assert!(dead.insert_events(std::slice::from_ref(&ev)).await.is_err());
        assert!(dead.apply_schema().await.is_err());

        let nan_ev = ClickHouseEvent { lcp: Some(f64::NAN), ..ev.clone() };
        assert!(dead.insert_events(&[nan_ev]).await.is_err());

        let trunc_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let trunc_port = trunc_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = trunc_listener.accept().await.unwrap();
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\nconnection: close\r\n\r\nshort")
                .await;
        });
        let trunc = ClickHouseClient::from_url(&format!("http://127.0.0.1:{trunc_port}/db")).unwrap();
        let trunc_rows: Result<Vec<serde_json::Value>, _> = trunc.query_json("SELECT 1").await;
        assert!(trunc_rows.is_err());

        let bad_json_app = Router::new().route(
            "/",
            post(|| async { "this is not json\n".into_response() }),
        );
        let (bad_port, bad_tx, bad_handle) = spawn_test_server(bad_json_app).await;
        let bad_json =
            ClickHouseClient::from_url(&format!("http://127.0.0.1:{bad_port}/db")).unwrap();
        let bad_rows: Result<Vec<serde_json::Value>, _> =
            bad_json.query_json("SELECT BADJSON").await;
        assert!(bad_rows.is_err());
        let _ = bad_tx.send(());
        let _ = bad_handle.await;

        let events_fail_app = Router::new().route(
            "/",
            post(|body: String| async move {
                if body.contains("CREATE DATABASE") {
                    "OK\n".into_response()
                } else {
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "boom",
                    )
                        .into_response()
                }
            }),
        );
        let (events_fail_port, events_fail_tx, events_fail_handle) =
            spawn_test_server(events_fail_app).await;
        let events_fail =
            ClickHouseClient::from_url(&format!("http://127.0.0.1:{events_fail_port}/db")).unwrap();
        assert!(events_fail.apply_schema().await.is_err());
        let _ = events_fail_tx.send(());
        let _ = events_fail_handle.await;

        let data_fail_app = Router::new().route(
            "/",
            post(|body: String| async move {
                if body.contains("CREATE DATABASE") || body.contains("PARTITION BY") {
                    "OK\n".into_response()
                } else {
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "boom",
                    )
                        .into_response()
                }
            }),
        );
        let (data_fail_port, data_fail_tx, data_fail_handle) =
            spawn_test_server(data_fail_app).await;
        let data_fail =
            ClickHouseClient::from_url(&format!("http://127.0.0.1:{data_fail_port}/db")).unwrap();
        assert!(data_fail.apply_schema().await.is_err());
        let _ = data_fail_tx.send(());
        let _ = data_fail_handle.await;
    }
}
