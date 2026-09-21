#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use kombu_core::detect::{get_device, normalize_browser, normalize_os};
use kombu_core::geo::get_region_code;
use kombu_ingest::{
    CollectPayload, generate_session_id, generate_visit_id, get_source_id, normalize_payload,
    validate_payload,
};

use crate::router::AppState;

pub fn collect_rate_limit(raw: Option<String>) -> u32 {
    raw.map_or(3000, |v| v.parse::<u32>().unwrap_or(3000))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRange {
    #[serde(alias = "start_at")]
    pub start_at: Option<i64>,
    #[serde(alias = "end_at")]
    pub end_at: Option<i64>,
    pub unit: Option<String>,
    pub r#type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub search: Option<String>,
    pub timezone: Option<String>,
    pub compare: Option<String>,
}

pub async fn send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CollectPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let client_ip = crate::rate_limit::extract_ip(&headers);
    let limit = collect_rate_limit(std::env::var("COLLECT_RATE_LIMIT").ok());
    let rate_key = format!("ingest:{client_ip}");
    if let Err((status, _, json_err)) = state.rate_limiter.check(&rate_key, limit, 60) {
        return Err((status, json_err));
    }

    let mut normalized = normalize_payload(body.payload);
    let source_id = get_source_id(&normalized)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))))?;
    if let Err(e) = validate_payload(&normalized) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": e }))));
    }

    let is_performance = body.r#type == "performance"
        || normalized.lcp.is_some()
        || normalized.inp.is_some()
        || normalized.cls.is_some()
        || normalized.fcp.is_some()
        || normalized.ttfb.is_some();
    let is_error = body.r#type == "error"
        || normalized.event_type == Some(kombu_core::constants::EVENT_TYPE_ERROR)
        || normalized.message.is_some()
        || normalized.stack.is_some();
    let event_type = normalized.event_type.unwrap_or_else(|| {
        kombu_core::types::determine_event_type(
            normalized.link.is_some(),
            normalized.pixel.is_some(),
            is_performance,
            is_error,
            normalized.name.is_some() || normalized.message.is_some(),
        )
    });
    normalized.event_type = Some(event_type);

    let website_exists = if let Some(exists) = state.website_cache.get(&source_id) {
        exists
    } else {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(SELECT 1 FROM "website" WHERE website_id = $1 AND deleted_at IS NULL)"#,
        )
        .bind(source_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        state.website_cache.insert(source_id, exists);
        exists
    };

    if !website_exists {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Website not found" })),
        ));
    }

    let custom_ip_header = std::env::var("CLIENT_IP_HEADER").ok();
    let header_iter = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.as_str(), s)));
    let ip_resolved = kombu_core::ip::resolve_client_ip(header_iter, custom_ip_header.as_deref())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let ip = ip_resolved.as_str();

    let ignore_test_header = headers
        .get("x-kombu-test-ignore-ip")
        .and_then(|v| v.to_str().ok());
    if kombu_core::ip::is_ip_ignored(ip, ignore_test_header) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "IP blocked" })),
        ));
    }

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let created_at = normalized
        .timestamp
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(Utc::now);

    let session_id = generate_session_id(source_id, ip, user_agent, &state.app_secret, created_at);
    let visit_id = generate_visit_id(session_id, created_at);

    let screen = normalized.screen.as_deref().unwrap_or("");
    let device = get_device(user_agent, screen).to_string();
    let browser = normalize_browser(normalized.browser.as_deref());
    let os = normalize_os(normalized.os.as_deref());

    let country = headers
        .get("cf-ipcountry")
        .or_else(|| headers.get("x-kombu-client-country"))
        .or_else(|| headers.get("x-umami-client-country"))
        .and_then(|v| v.to_str().ok());

    let raw_region = headers
        .get("cf-region-code")
        .or_else(|| headers.get("x-kombu-client-region"))
        .or_else(|| headers.get("x-umami-client-region"))
        .and_then(|v| v.to_str().ok());

    let region = get_region_code(country, raw_region);

    let city = headers
        .get("cf-ipcity")
        .or_else(|| headers.get("x-kombu-client-city"))
        .or_else(|| headers.get("x-umami-client-city"))
        .and_then(|v| v.to_str().ok());

    let session_known = state.session_cache.get(&session_id).unwrap_or(false);

    let ua_str = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .or(normalized.user_agent.as_deref());

    let bot_info = kombu_core::bot::classify_bot(
        ua_str,
        normalized.screen.as_deref(),
        normalized.referrer.as_deref(),
        normalized.webdriver.unwrap_or(false),
    );
    let is_bot = normalized.is_bot.unwrap_or(bot_info.is_bot);
    let bot_score = normalized.bot_score.unwrap_or(bot_info.score);

    let ingest_item = crate::queue::IngestItem {
        source_id,
        session_id,
        visit_id,
        data: normalized,
        created_at,
        device,
        browser,
        os,
        country: country.map(str::to_string),
        region,
        city: city.map(str::to_string),
        session_known_exists: session_known,
        is_bot,
        bot_score,
    };

    if !session_known {
        state.session_cache.insert(session_id, true);
    }

    let _ = state.ingest_queue.try_push(ingest_item);

    let cache_token = format!("{session_id}:{visit_id}");
    Ok(Json(json!({ "cache": cache_token })))
}

pub async fn batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let client_ip = crate::rate_limit::extract_ip(&headers);
    let limit = collect_rate_limit(std::env::var("COLLECT_RATE_LIMIT").ok());
    let rate_key = format!("ingest:{client_ip}");
    if let Err((status, _, json_err)) = state.rate_limiter.check(&rate_key, limit, 60) {
        return Err((status, json_err));
    }

    if let Some(arr) = body.as_array() {
        for item in arr {
            if let Ok(payload) = serde_json::from_value::<CollectPayload>(item.clone()) {
                let _ = send(State(state.clone()), headers.clone(), Json(payload)).await;
            }
        }
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize, Default)]
pub struct HeartbeatPayload {
    pub website: Option<String>,
    pub url: Option<String>,
    pub hostname: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "visitId")]
    pub visit_id: Option<String>,
}

pub async fn heartbeat() -> Json<Value> {
    Json(json!({ "ok": true, "status": "alive" }))
}

pub async fn heartbeat_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<HeartbeatPayload>>,
) -> Json<Value> {
    if let Some(Json(body)) = body {
        if let Some(website_str) = body.website {
            if let Ok(source_id) = Uuid::parse_str(&website_str) {
                let website_exists = if let Some(exists) = state.website_cache.get(&source_id) {
                    exists
                } else {
                    let exists = sqlx::query_scalar::<_, bool>(
                        r#"SELECT EXISTS(SELECT 1 FROM "website" WHERE website_id = $1 AND deleted_at IS NULL)"#,
                    )
                    .bind(source_id)
                    .fetch_one(&state.pool)
                    .await
                    .unwrap_or(false);

                    state.website_cache.insert(source_id, exists);
                    exists
                };

                if !website_exists {
                    return Json(json!({ "ok": false, "error": "Website not found" }));
                }

                let custom_ip_header = std::env::var("CLIENT_IP_HEADER").ok();
                let header_iter = headers
                    .iter()
                    .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.as_str(), s)));
                let ip =
                    kombu_core::ip::resolve_client_ip(header_iter, custom_ip_header.as_deref())
                        .unwrap_or_else(|| "127.0.0.1".to_string());

                let ignore_test_header = headers
                    .get("x-kombu-test-ignore-ip")
                    .and_then(|v| v.to_str().ok());
                if kombu_core::ip::is_ip_ignored(&ip, ignore_test_header) {
                    return Json(json!({ "ok": false, "error": "IP blocked" }));
                }

                let user_agent = headers
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                let now = Utc::now();
                let session_id = body
                    .session_id
                    .and_then(|s| Uuid::parse_str(&s).ok())
                    .unwrap_or_else(|| {
                        generate_session_id(source_id, &ip, user_agent, &state.app_secret, now)
                    });
                let visit_id = body
                    .visit_id
                    .and_then(|s| Uuid::parse_str(&s).ok())
                    .unwrap_or_else(|| generate_visit_id(session_id, now));

                let normalized = kombu_ingest::CollectData {
                    website: Some(source_id.to_string()),
                    url: body.url.or_else(|| Some("/".to_string())),
                    hostname: body.hostname,
                    name: Some("heartbeat".to_string()),
                    ip: Some(ip),
                    user_agent: Some(user_agent.to_string()),
                    event_type: Some(kombu_core::constants::EVENT_TYPE_CUSTOM_EVENT),
                    ..Default::default()
                };

                let item = crate::queue::IngestItem {
                    source_id,
                    session_id,
                    visit_id,
                    data: normalized,
                    created_at: now,
                    device: "desktop".to_string(),
                    browser: None,
                    os: None,
                    country: None,
                    region: None,
                    city: None,
                    session_known_exists: true,
                    is_bot: false,
                    bot_score: 0,
                };
                let _ = state.ingest_queue.try_push(item);
            }
        }
    }

    Json(json!({ "ok": true, "status": "alive" }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use kombu_ingest::CollectData;
    use uuid::Uuid;

    #[test]
    fn test_collect_rate_limit_helper() {
        assert_eq!(collect_rate_limit(None), 3000);
        assert_eq!(collect_rate_limit(Some("2500".into())), 2500);
        assert_eq!(collect_rate_limit(Some("not-a-number".into())), 3000);
    }

    #[tokio::test]
    async fn test_send_validate_rejection() {
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
        sqlx::query(
            r#"INSERT INTO "website" (website_id, name, domain, created_at, updated_at) VALUES ($1, 'FormulaSite', 'formula.test', NOW(), NOW())"#,
        )
        .bind(website_id)
        .execute(&pool)
        .await
        .unwrap();
        let formula_payload = CollectPayload {
            r#type: "event".into(),
            payload: CollectData {
                website: Some(website_id.to_string()),
                url: Some("/home".into()),
                name: Some("=cmd|' /C calc'!A0".into()),
                ..Default::default()
            },
        };
        let res = send(
            State(state.clone()),
            HeaderMap::new(),
            Json(formula_payload),
        )
        .await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, StatusCode::BAD_REQUEST);

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
    }

    #[test]
    fn test_query_range_deserialize() {
        let json_data = json!({
            "startAt": 1_700_000_000,
            "endAt": 1_700_086_400,
            "unit": "day",
            "type": "event",
            "limit": 50,
            "search": "test",
            "timezone": "UTC",
            "compare": "prev"
        });
        let qr: QueryRange = serde_json::from_value(json_data).unwrap();
        assert_eq!(qr.start_at, Some(1_700_000_000));
        assert_eq!(qr.end_at, Some(1_700_086_400));
        assert_eq!(qr.unit.as_deref(), Some("day"));
        assert_eq!(qr.r#type.as_deref(), Some("event"));
        assert_eq!(qr.limit, Some(50));
        assert_eq!(qr.search.as_deref(), Some("test"));
        assert_eq!(qr.timezone.as_deref(), Some("UTC"));
        assert_eq!(qr.compare.as_deref(), Some("prev"));

        let json_aliases = json!({
            "start_at": 1_700_000_000,
            "end_at": 1_700_086_400
        });
        let qr_alias: QueryRange = serde_json::from_value(json_aliases).unwrap();
        assert_eq!(qr_alias.start_at, Some(1_700_000_000));
        assert_eq!(qr_alias.end_at, Some(1_700_086_400));
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let res = heartbeat().await;
        assert_eq!(res.0["ok"], true);

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

        let res_none = heartbeat_post(State(state.clone()), HeaderMap::new(), None).await;
        assert_eq!(res_none.0["ok"], true);

        let res_no_web = heartbeat_post(
            State(state.clone()),
            HeaderMap::new(),
            Some(Json(HeartbeatPayload {
                website: None,
                url: None,
                hostname: None,
                session_id: None,
                visit_id: None,
            })),
        )
        .await;
        assert_eq!(res_no_web.0["ok"], true);

        let res_bad_uuid = heartbeat_post(
            State(state.clone()),
            HeaderMap::new(),
            Some(Json(HeartbeatPayload {
                website: Some("not-a-uuid".into()),
                url: None,
                hostname: None,
                session_id: None,
                visit_id: None,
            })),
        )
        .await;
        assert_eq!(res_bad_uuid.0["ok"], true);

        let missing_id = Uuid::now_v7();
        let res_missing = heartbeat_post(
            State(state.clone()),
            HeaderMap::new(),
            Some(Json(HeartbeatPayload {
                website: Some(missing_id.to_string()),
                url: None,
                hostname: None,
                session_id: None,
                visit_id: None,
            })),
        )
        .await;
        assert_eq!(res_missing.0["ok"], false);

        let site_id = Uuid::now_v7();
        let _ = sqlx::query(
            r#"INSERT INTO "website" (website_id, name, domain) VALUES ($1, 'HB Site', 'hb.site')"#,
        )
        .bind(site_id)
        .execute(&pool)
        .await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-kombu-test-ignore-ip",
            HeaderValue::from_static("127.0.0.1"),
        );
        let res_blocked = heartbeat_post(
            State(state.clone()),
            headers,
            Some(Json(HeartbeatPayload {
                website: Some(site_id.to_string()),
                url: Some("/hb".into()),
                hostname: Some("hb.site".into()),
                session_id: None,
                visit_id: None,
            })),
        )
        .await;
        assert_eq!(res_blocked.0["ok"], false);

        let sess_id = Uuid::now_v7();
        let vis_id = Uuid::now_v7();
        let res_ok = heartbeat_post(
            State(state.clone()),
            HeaderMap::new(),
            Some(Json(HeartbeatPayload {
                website: Some(site_id.to_string()),
                url: Some("/hb".into()),
                hostname: Some("hb.site".into()),
                session_id: Some(sess_id.to_string()),
                visit_id: Some(vis_id.to_string()),
            })),
        )
        .await;
        assert_eq!(res_ok.0["ok"], true);

        let mut headers_bad_ua = HeaderMap::new();
        headers_bad_ua.insert(
            "user-agent",
            axum::http::HeaderValue::from_bytes(&[0xFF]).unwrap(),
        );
        let res_bad_ua = heartbeat_post(
            State(state.clone()),
            headers_bad_ua,
            Some(Json(HeartbeatPayload {
                website: Some(site_id.to_string()),
                url: Some("/hb".into()),
                hostname: Some("hb.site".into()),
                session_id: Some(sess_id.to_string()),
                visit_id: Some(vis_id.to_string()),
            })),
        )
        .await;
        assert_eq!(res_bad_ua.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(site_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_send_and_batch_coverage() {
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

        let bad_source = CollectPayload {
            r#type: "event".into(),
            payload: CollectData {
                website: None,
                link: None,
                pixel: None,
                ..Default::default()
            },
        };
        let res_no_source = send(State(state.clone()), HeaderMap::new(), Json(bad_source)).await;
        assert!(res_no_source.is_err());
        assert_eq!(res_no_source.unwrap_err().0, StatusCode::BAD_REQUEST);

        let bad_payload = CollectPayload {
            r#type: "event".into(),
            payload: CollectData {
                website: Some("=cmd|' /C calc'!A0".into()),
                ..Default::default()
            },
        };
        let res_bad = send(State(state.clone()), HeaderMap::new(), Json(bad_payload)).await;
        assert!(res_bad.is_err());
        assert_eq!(res_bad.unwrap_err().0, StatusCode::BAD_REQUEST);

        let missing_site_id = Uuid::now_v7();
        let missing_site_payload = CollectPayload {
            r#type: "event".into(),
            payload: CollectData {
                website: Some(missing_site_id.to_string()),
                url: Some("/home".into()),
                ..Default::default()
            },
        };
        let res_missing = send(
            State(state.clone()),
            HeaderMap::new(),
            Json(missing_site_payload),
        )
        .await;
        assert!(res_missing.is_err());
        assert_eq!(res_missing.unwrap_err().0, StatusCode::BAD_REQUEST);

        let website_id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO "website" (website_id, name, domain, created_at, updated_at) VALUES ($1, 'IngestTestSite', 'ingest.test', NOW(), NOW())"#,
        )
        .bind(website_id)
        .execute(&pool)
        .await
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("8.8.8.8, 1.1.1.1"),
        );
        headers.insert(
            "user-agent",
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0 Safari/537.36",
            ),
        );
        headers.insert("cf-ipcountry", HeaderValue::from_static("US"));
        headers.insert("cf-region-code", HeaderValue::from_static("CA"));
        headers.insert("cf-ipcity", HeaderValue::from_static("Los Angeles"));

        let valid_payload = CollectPayload {
            r#type: "event".into(),
            payload: CollectData {
                website: Some(website_id.to_string()),
                hostname: Some("ingest.test".into()),
                url: Some("/blog/article-1?utm_source=twitter".into()),
                title: Some("Article 1".into()),
                screen: Some("1920x1080".into()),
                browser: Some("Chrome".into()),
                os: Some("Windows".into()),
                device: Some("desktop".into()),
                language: Some("en-US".into()),
                timestamp: Some(Utc::now().timestamp()),
                ..Default::default()
            },
        };

        let res_send_1 = send(
            State(state.clone()),
            headers.clone(),
            Json(valid_payload.clone()),
        )
        .await
        .unwrap();
        assert!(res_send_1.0["cache"].is_string());

        let res_send_2 = send(
            State(state.clone()),
            headers.clone(),
            Json(valid_payload.clone()),
        )
        .await
        .unwrap();
        assert!(res_send_2.0["cache"].is_string());

        let perf_payload = CollectPayload {
            r#type: "performance".into(),
            payload: CollectData {
                website: Some(website_id.to_string()),
                url: Some("/dashboard".into()),
                lcp: Some(1.2),
                inp: Some(40.0),
                cls: Some(0.02),
                fcp: Some(0.8),
                ttfb: Some(0.2),
                ..Default::default()
            },
        };
        let res_perf = send(State(state.clone()), headers.clone(), Json(perf_payload))
            .await
            .unwrap();
        assert!(res_perf.0["cache"].is_string());

        let err_payload = CollectPayload {
            r#type: "error".into(),
            payload: CollectData {
                website: Some(website_id.to_string()),
                url: Some("/checkout".into()),
                message: Some("Payment gateway timed out".into()),
                stack: Some("Error at checkout.js:42".into()),
                ..Default::default()
            },
        };
        let res_err = send(State(state.clone()), headers.clone(), Json(err_payload))
            .await
            .unwrap();
        assert!(res_err.0["cache"].is_string());

        let batch_events = json!([
            {
                "type": "event",
                "payload": {
                    "website": website_id.to_string(),
                    "url": "/page-1"
                }
            },
            {
                "type": "event",
                "payload": {
                    "website": website_id.to_string(),
                    "url": "/page-2"
                }
            },
            {
                "invalid": "structure"
            }
        ]);
        let res_batch = batch(State(state.clone()), headers.clone(), Json(batch_events))
            .await
            .unwrap();
        assert_eq!(res_batch.0["ok"], true);

        let res_batch_non_array = batch(
            State(state.clone()),
            headers.clone(),
            Json(json!({ "not": "an array" })),
        )
        .await
        .unwrap();
        assert_eq!(res_batch_non_array.0["ok"], true);

        let mut blocked_headers = HeaderMap::new();
        blocked_headers.insert(
            "x-kombu-test-ignore-ip",
            HeaderValue::from_static("127.0.0.1"),
        );
        let res_blocked = send(
            State(state.clone()),
            blocked_headers.clone(),
            Json(valid_payload.clone()),
        )
        .await;
        assert!(res_blocked.is_err());
        assert_eq!(res_blocked.unwrap_err().0, StatusCode::FORBIDDEN);

        for _ in 0..3001 {
            let _ = state.rate_limiter.check("ingest:127.0.0.1", 3000, 60);
        }
        let res_rl_send = send(
            State(state.clone()),
            HeaderMap::new(),
            Json(valid_payload.clone()),
        )
        .await;
        assert!(res_rl_send.is_err());
        let res_rl_batch = batch(
            State(state.clone()),
            HeaderMap::new(),
            Json(json!([valid_payload.clone()])),
        )
        .await;
        assert!(res_rl_batch.is_err());

        let (closed_tx, rx) = tokio::sync::mpsc::channel(1);
        drop(rx);
        let closed_queue_state = AppState {
            pool: pool.clone(),
            session_cache: state.session_cache.clone(),
            website_cache: state.website_cache.clone(),
            rate_limiter: crate::rate_limit::RateLimiter::new(),
            ingest_queue: crate::queue::IngestQueue {
                senders: vec![closed_tx],
                mask: 0,
            },
            app_secret: state.app_secret.clone(),
        };
        let res_closed_q = send(
            State(closed_queue_state),
            HeaderMap::new(),
            Json(valid_payload.clone()),
        )
        .await;
        assert!(res_closed_q.is_ok());

        let res_hb_none = heartbeat_post(State(state.clone()), HeaderMap::new(), None).await;
        assert_eq!(res_hb_none.0["ok"], true);

        let res_hb_missing = heartbeat_post(
            State(state.clone()),
            HeaderMap::new(),
            Some(Json(HeartbeatPayload {
                website: Some(Uuid::now_v7().to_string()),
                ..Default::default()
            })),
        )
        .await;
        assert_eq!(res_hb_missing.0["ok"], false);

        let res_hb_blocked = heartbeat_post(
            State(state.clone()),
            blocked_headers.clone(),
            Some(Json(HeartbeatPayload {
                website: Some(website_id.to_string()),
                ..Default::default()
            })),
        )
        .await;
        assert_eq!(res_hb_blocked.0["ok"], false);

        let res_hb_ok = heartbeat_post(
            State(state.clone()),
            HeaderMap::new(),
            Some(Json(HeartbeatPayload {
                website: Some(website_id.to_string()),
                ..Default::default()
            })),
        )
        .await;
        assert_eq!(res_hb_ok.0["ok"], true);

        let mut batch_headers = HeaderMap::new();
        batch_headers.insert("x-forwarded-for", "123.45.67.89".parse().unwrap());
        let batch_body = json!([
            valid_payload.clone(),
            { "invalid": true }
        ]);
        let res_batch = batch(State(state.clone()), batch_headers, Json(batch_body)).await;
        assert!(res_batch.is_ok());

        let res_hb_alive = heartbeat().await;
        assert_eq!(res_hb_alive.0["status"], "alive");

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
    }
}
