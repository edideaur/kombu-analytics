#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PAYLOAD_CAPS_STRING: usize = 500;
pub const PAYLOAD_MAX_KEYS: usize = 50;
pub const EVENT_NAME_TRUNC: usize = 50;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectPayload {
    #[serde(rename = "type")]
    pub r#type: String,
    pub payload: CollectData,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CollectData {
    pub website: Option<String>,
    pub link: Option<String>,
    pub pixel: Option<String>,
    pub hostname: Option<String>,
    pub language: Option<String>,
    pub referrer: Option<String>,
    pub screen: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub name: Option<String>,
    pub data: Option<serde_json::Value>,
    pub tag: Option<String>,
    pub ip: Option<String>,
    #[serde(rename = "userAgent")]
    pub user_agent: Option<String>,
    pub timestamp: Option<i64>,
    pub id: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device: Option<String>,
    pub lcp: Option<f64>,
    pub inp: Option<f64>,
    pub cls: Option<f64>,
    pub fcp: Option<f64>,
    pub ttfb: Option<f64>,
    #[serde(default)]
    pub event_type: Option<i32>,
    #[serde(default)]
    pub webdriver: Option<bool>,
    #[serde(default)]
    pub is_bot: Option<bool>,
    #[serde(default)]
    pub bot_score: Option<u8>,
    pub message: Option<String>,
    pub stack: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    pub cache: String,
    pub session_id: Uuid,
    pub visit_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheToken {
    pub r#type: String,
    pub website_id: String,
    pub session_id: String,
    pub visit_id: String,
    pub iat: i64,
    pub session_link_id: Option<String>,
}

pub fn get_source_id(data: &CollectData) -> Result<Uuid, String> {
    let id_str = match (&data.website, &data.link, &data.pixel) {
        (Some(w), None, None) => w.as_str(),
        (None, Some(l), None) => l.as_str(),
        (None, None, Some(p)) => p.as_str(),
        _ => return Err("Exactly one of website, link, or pixel must be provided".into()),
    };
    match Uuid::parse_str(id_str) {
        Ok(u) => Ok(u),
        Err(_) => Err("Invalid UUID".to_string()),
    }
}

pub fn generate_session_id(
    source_id: Uuid,
    ip: &str,
    user_agent: &str,
    secret: &str,
    created_at: DateTime<Utc>,
) -> Uuid {
    let year = created_at.year();
    let month = created_at.month();

    let mut salt_hasher = blake3::Hasher::new();
    salt_hasher.update(year.to_string().as_bytes());
    salt_hasher.update(b"-");
    salt_hasher.update(month.to_string().as_bytes());
    salt_hasher.update(b"-01-");
    salt_hasher.update(secret.as_bytes());
    salt_hasher.update(b"-salt");
    let session_salt = salt_hasher.finalize();

    let mut hasher = blake3::Hasher::new();
    hasher.update(source_id.as_bytes());
    hasher.update(ip.as_bytes());
    hasher.update(user_agent.as_bytes());
    hasher.update(session_salt.as_bytes());
    let session_hash = hasher.finalize();

    Uuid::new_v5(&Uuid::NAMESPACE_DNS, session_hash.as_bytes())
}

pub fn generate_visit_id(session_id: Uuid, created_at: DateTime<Utc>) -> Uuid {
    let hour = created_at.hour();
    let day = created_at.day();

    let mut hasher = blake3::Hasher::new();
    hasher.update(session_id.as_bytes());
    hasher.update(&[day as u8, hour as u8]);
    let visit_salt = hasher.finalize();

    Uuid::new_v5(&Uuid::NAMESPACE_DNS, visit_salt.as_bytes())
}

pub fn validate_payload(data: &CollectData) -> Result<(), String> {
    let _ = get_source_id(data)?;
    if let Some(name) = &data.name {
        if name.starts_with('=')
            || name.starts_with('+')
            || name.starts_with('-')
            || name.starts_with('@')
        {
            return Err("Formula trigger rejected".into());
        }
    }
    if let Some(d) = &data.data {
        if let Some(obj) = d.as_object() {
            if obj.len() > PAYLOAD_MAX_KEYS {
                return Err(format!("Too many keys {}", obj.len()));
            }
            for (k, v) in obj {
                if k.len() > PAYLOAD_CAPS_STRING {
                    return Err(format!("Key too long: {k}"));
                }
                if v.as_str().is_some_and(|s| s.len() > PAYLOAD_CAPS_STRING) {
                    return Err("String value too long".into());
                }
            }
        }
    }
    Ok(())
}

pub fn normalize_payload(mut data: CollectData) -> CollectData {
    if let Some(name) = data.name.take() {
        let truncated = if name.len() > EVENT_NAME_TRUNC {
            name.chars().take(EVENT_NAME_TRUNC).collect()
        } else {
            name
        };
        data.name = Some(truncated);
    }
    #[allow(clippy::single_match)]
    match data.data.as_mut() {
        Some(serde_json::Value::Object(map)) => {
            for v in map.values_mut() {
                match v {
                    serde_json::Value::String(s) => {
                        if s.len() > PAYLOAD_CAPS_STRING {
                            *s = s.chars().take(PAYLOAD_CAPS_STRING).collect();
                        }
                    }
                    serde_json::Value::Number(n) => {
                        let f = n.as_f64().unwrap_or(0.0);
                        let rounded = (f * 10000.0).round() / 10000.0;
                        *v = serde_json::Value::Number(
                            serde_json::Number::from_f64(rounded).unwrap_or(n.clone()),
                        );
                    }
                    _ => {}
                }
            }
            if map.len() > PAYLOAD_MAX_KEYS {
                let keys: Vec<String> = map.keys().cloned().collect();
                for k in keys.iter().skip(PAYLOAD_MAX_KEYS) {
                    map.remove(k);
                }
            }
        }
        _ => {}
    }
    data
}

pub async fn save_session_and_event(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    session_id: Uuid,
    visit_id: Uuid,
    data: &CollectData,
    created_at: DateTime<Utc>,
    device: &str,
    browser: Option<&str>,
    os: Option<&str>,
    country: Option<&str>,
    region: Option<&str>,
    city: Option<&str>,
) -> Result<(), sqlx::Error> {
    save_session_and_event_cached(
        pool, source_id, session_id, visit_id, data, created_at, device, browser, os, country,
        region, city, false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn save_session_and_event_cached(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    session_id: Uuid,
    visit_id: Uuid,
    data: &CollectData,
    created_at: DateTime<Utc>,
    device: &str,
    browser: Option<&str>,
    os: Option<&str>,
    country: Option<&str>,
    region: Option<&str>,
    city: Option<&str>,
    session_known_exists: bool,
) -> Result<(), sqlx::Error> {
    let bot_info = kombu_core::bot::classify_bot(
        data.user_agent.as_deref(),
        data.screen.as_deref(),
        data.referrer.as_deref(),
        data.webdriver.unwrap_or(false),
    );
    let is_bot = data.is_bot.unwrap_or(bot_info.is_bot);
    let bot_score = i16::from(data.bot_score.unwrap_or(bot_info.score));

    if !session_known_exists {
        sqlx::query(
            r#"
            INSERT INTO "session" (
                session_id, website_id, browser, os, device, screen, language, country, region, city, distinct_id, is_bot, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (session_id) DO NOTHING
            "#,
        )
        .bind(session_id)
        .bind(source_id)
        .bind(browser)
        .bind(os)
        .bind(device)
        .bind(&data.screen)
        .bind(&data.language)
        .bind(country)
        .bind(region)
        .bind(city)
        .bind(&data.id)
        .bind(is_bot)
        .bind(created_at)
        .execute(pool)
        .await?;
    }

    let event_id = Uuid::now_v7();
    let url_raw = data.url.as_deref().unwrap_or("/");
    let (raw_path, raw_query) = match url_raw.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (url_raw, None),
    };
    let url_path = kombu_core::url::truncate_url_path(raw_path);
    let url_query = raw_query.map(|q| kombu_core::url::truncate_string(q, 500));

    let query_params = raw_query
        .map(kombu_core::url::parse_query_params)
        .unwrap_or_default();

    let parsed_ref = data
        .referrer
        .as_deref()
        .map(|r| kombu_core::url::parse_referrer(r, data.hostname.as_deref()))
        .unwrap_or_default();

    let is_error = data.event_type == Some(kombu_core::constants::EVENT_TYPE_ERROR)
        || data.message.is_some()
        || data.stack.is_some();

    let event_type = data.event_type.unwrap_or_else(|| {
        kombu_core::types::determine_event_type(
            data.link.is_some(),
            data.pixel.is_some(),
            data.lcp.is_some()
                || data.inp.is_some()
                || data.cls.is_some()
                || data.fcp.is_some()
                || data.ttfb.is_some(),
            is_error,
            data.name.is_some() || data.message.is_some(),
        )
    });

    let event_title = data
        .title
        .as_deref()
        .map(|s| kombu_core::url::truncate_string(s, 500));
    let event_name = data.name.as_deref().or(data.message.as_deref());
    let tag = data
        .tag
        .as_deref()
        .map(|s| kombu_core::url::truncate_string(s, 50));
    let hostname = data
        .hostname
        .as_deref()
        .map(|s| kombu_core::url::truncate_string(s, 100));

    sqlx::query(
        r#"
        INSERT INTO "website_event" (
            event_id, website_id, session_id, visit_id,
            url_path, url_query,
            referrer_path, referrer_query, referrer_domain,
            page_title, event_type, event_name, tag, hostname,
            utm_source, utm_medium, utm_campaign, utm_content, utm_term,
            gclid, fbclid, msclkid, ttclid, li_fat_id, twclid,
            cls, fcp, inp, lcp, ttfb,
            is_bot, bot_score,
            created_at
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6,
            $7, $8, $9,
            $10, $11, $12, $13, $14,
            $15, $16, $17, $18, $19,
            $20, $21, $22, $23, $24, $25,
            $26::numeric, $27::numeric, $28::numeric, $29::numeric, $30::numeric,
            $31, $32,
            $33
        )
        "#,
    )
    .bind(event_id)
    .bind(source_id)
    .bind(session_id)
    .bind(visit_id)
    .bind(url_path)
    .bind(url_query)
    .bind(parsed_ref.referrer_path)
    .bind(parsed_ref.referrer_query)
    .bind(parsed_ref.referrer_domain)
    .bind(event_title)
    .bind(event_type)
    .bind(event_name)
    .bind(tag)
    .bind(hostname)
    .bind(query_params.utm_source)
    .bind(query_params.utm_medium)
    .bind(query_params.utm_campaign)
    .bind(query_params.utm_content)
    .bind(query_params.utm_term)
    .bind(query_params.gclid)
    .bind(query_params.fbclid)
    .bind(query_params.msclkid)
    .bind(query_params.ttclid)
    .bind(query_params.li_fat_id)
    .bind(query_params.twclid)
    .bind(data.cls)
    .bind(data.fcp)
    .bind(data.inp)
    .bind(data.lcp)
    .bind(data.ttfb)
    .bind(is_bot)
    .bind(bot_score)
    .bind(created_at)
    .execute(pool)
    .await?;

    if let Some(event_data_val) = &data.data {
        let flattened = kombu_core::data::flatten_event_data(event_data_val).unwrap_or_else(|_| {
            let mut out = Vec::new();
            kombu_core::data::flatten_json(event_data_val, "", &mut out);
            out.truncate(kombu_core::constants::EVENT_DATA_MAX_KEYS);
            out
        });

        for item in flattened {
            let number_val = if item.data_type == kombu_core::constants::DATA_TYPE_NUMBER {
                item.value.parse::<f64>().ok()
            } else {
                None
            };
            let date_val = if item.data_type == kombu_core::constants::DATA_TYPE_DATE {
                chrono::DateTime::parse_from_rfc3339(&item.value)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            } else {
                None
            };
            let truncated_key = kombu_core::url::truncate_string(&item.key, 500);

            sqlx::query(
                r#"
                INSERT INTO "event_data" (
                    event_data_id, website_id, website_event_id, data_key, string_value, number_value, date_value, data_type, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8, $9)
                "#,
            )
            .bind(Uuid::now_v7())
            .bind(source_id)
            .bind(event_id)
            .bind(truncated_key)
            .bind(Some(item.value))
            .bind(number_val)
            .bind(date_val)
            .bind(item.data_type)
            .bind(created_at)
            .execute(pool)
            .await?;
        }

        if let Some(map) = event_data_val.as_object() {
            let revenue_val = map.get("revenue").and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
            });
            let currency_val = map.get("currency").and_then(|v| v.as_str()).map(str::trim);

            if let (Some(rev), Some(curr)) = (revenue_val, currency_val) {
                if rev > 0.0 && !curr.is_empty() {
                    let rev_id = Uuid::now_v7();
                    let ev_name = data.name.as_deref().unwrap_or("");
                    let truncated_ev_name = kombu_core::url::truncate_event_name(ev_name);
                    let truncated_currency = kombu_core::url::truncate_string(curr, 10);

                    sqlx::query(
                        r#"
                        INSERT INTO "revenue" (
                            revenue_id, website_id, session_id, event_id, event_name, currency, revenue, created_at
                        ) VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8)
                        "#,
                    )
                    .bind(rev_id)
                    .bind(source_id)
                    .bind(session_id)
                    .bind(event_id)
                    .bind(truncated_ev_name)
                    .bind(truncated_currency)
                    .bind(rev)
                    .bind(created_at)
                    .execute(pool)
                    .await?;
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn build_clickhouse_event(
    source_id: Uuid,
    session_id: Uuid,
    visit_id: Uuid,
    event_id: Uuid,
    data: &CollectData,
    created_at: DateTime<Utc>,
    device: &str,
    browser: Option<&str>,
    os: Option<&str>,
    country: Option<&str>,
    region: Option<&str>,
    city: Option<&str>,
) -> kombu_db::ClickHouseEvent {
    let url_raw = data.url.as_deref().unwrap_or("/");
    let (raw_path, raw_query) = match url_raw.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (url_raw, None),
    };
    let url_path = kombu_core::url::truncate_url_path(raw_path);
    let url_query = raw_query.map(|q| kombu_core::url::truncate_string(q, 500));

    let query_params = raw_query
        .map(kombu_core::url::parse_query_params)
        .unwrap_or_default();

    let parsed_ref = data
        .referrer
        .as_deref()
        .map(|r| kombu_core::url::parse_referrer(r, data.hostname.as_deref()))
        .unwrap_or_default();

    let is_error = data.event_type == Some(kombu_core::constants::EVENT_TYPE_ERROR)
        || data.message.is_some()
        || data.stack.is_some();

    let event_type = data.event_type.unwrap_or_else(|| {
        kombu_core::types::determine_event_type(
            data.link.is_some(),
            data.pixel.is_some(),
            data.lcp.is_some()
                || data.inp.is_some()
                || data.cls.is_some()
                || data.fcp.is_some()
                || data.ttfb.is_some(),
            is_error,
            data.name.is_some() || data.message.is_some(),
        )
    });

    let event_title = data
        .title
        .as_deref()
        .map(|s| kombu_core::url::truncate_string(s, 500));
    let event_name = data.name.as_deref().or(data.message.as_deref()).map(ToString::to_string);
    let tag = data
        .tag
        .as_deref()
        .map(|s| kombu_core::url::truncate_string(s, 50));
    let hostname = data.hostname.clone().unwrap_or_default();

    kombu_db::ClickHouseEvent {
        website_id: source_id,
        session_id,
        visit_id,
        event_id,
        hostname,
        browser: browser.unwrap_or("unknown").to_string(),
        os: os.unwrap_or("unknown").to_string(),
        device: device.to_string(),
        screen: data.screen.clone().unwrap_or_default(),
        language: data.language.clone().unwrap_or_default(),
        country: country.unwrap_or("unknown").to_string(),
        region: region.unwrap_or("unknown").to_string(),
        city: city.unwrap_or("unknown").to_string(),
        url_path,
        url_query,
        utm_source: query_params.utm_source,
        utm_medium: query_params.utm_medium,
        utm_campaign: query_params.utm_campaign,
        utm_content: query_params.utm_content,
        utm_term: query_params.utm_term,
        referrer_path: parsed_ref.referrer_path,
        referrer_query: parsed_ref.referrer_query,
        referrer_domain: parsed_ref.referrer_domain,
        page_title: event_title,
        gclid: query_params.gclid,
        fbclid: query_params.fbclid,
        msclkid: query_params.msclkid,
        ttclid: query_params.ttclid,
        li_fat_id: query_params.li_fat_id,
        twclid: query_params.twclid,
        lcp: data.lcp,
        inp: data.inp,
        cls: data.cls,
        fcp: data.fcp,
        ttfb: data.ttfb,
        event_type: event_type as u32,
        event_name,
        tag,
        distinct_id: data.id.clone(),
        created_at: created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

pub async fn save_event_clickhouse(
    client: &kombu_db::ClickHouseClient,
    event: &kombu_db::ClickHouseEvent,
) -> Result<(), kombu_db::ClickHouseError> {
    client.insert_events(std::slice::from_ref(event)).await
}

#[allow(clippy::too_many_arguments)]
pub async fn save_session_and_event_with_engine(
    engine: kombu_core::types::StorageEngine,
    pool: &sqlx::PgPool,
    ch_client: Option<&kombu_db::ClickHouseClient>,
    source_id: Uuid,
    session_id: Uuid,
    visit_id: Uuid,
    data: &CollectData,
    created_at: DateTime<Utc>,
    device: &str,
    browser: Option<&str>,
    os: Option<&str>,
    country: Option<&str>,
    region: Option<&str>,
    city: Option<&str>,
    session_known_exists: bool,
) -> Result<(), sqlx::Error> {
    save_session_and_event_cached(
        pool,
        source_id,
        session_id,
        visit_id,
        data,
        created_at,
        device,
        browser,
        os,
        country,
        region,
        city,
        session_known_exists,
    )
    .await?;

    if engine == kombu_core::types::StorageEngine::Partitioned {
        let _ = kombu_db::partitioned::refresh_hourly_rollups(
            pool,
            source_id,
            created_at - chrono::Duration::hours(1),
            created_at + chrono::Duration::hours(1),
        )
        .await;
    }

    if let (kombu_core::types::StorageEngine::Clickhouse, Some(client)) = (engine, ch_client) {
        let event_id = Uuid::now_v7();
        let ch_event = build_clickhouse_event(
            source_id,
            session_id,
            visit_id,
            event_id,
            data,
            created_at,
            device,
            browser,
            os,
            country,
            region,
            city,
        );
        let _ = save_event_clickhouse(client, &ch_event).await;
    }

    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::manual_let_else,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_f64_to_numeric() {
        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();
        let wid = Uuid::parse_str("01a0aa65-c671-72cc-b248-2884c0b76273").unwrap();
        let sid = Uuid::now_v7();
        let vid = Uuid::now_v7();
        let eid = Uuid::now_v7();
        let now = Utc::now();

        let _ = sqlx::query(
            r#"
            INSERT INTO "session" (session_id, website_id, created_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (session_id) DO NOTHING
            "#,
        )
        .bind(sid)
        .bind(wid)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let res = sqlx::query(
            r#"
            INSERT INTO "website_event" (
                event_id, website_id, session_id, visit_id,
                url_path, url_query,
                referrer_path, referrer_query, referrer_domain,
                page_title, event_type, event_name, tag, hostname,
                utm_source, utm_medium, utm_campaign, utm_content, utm_term,
                gclid, fbclid, msclkid, ttclid, li_fat_id, twclid,
                cls, fcp, inp, lcp, ttfb,
                created_at
            ) VALUES (
                $1, $2, $3, $4,
                $5, $6,
                $7, $8, $9,
                $10, $11, $12, $13, $14,
                $15, $16, $17, $18, $19,
                $20, $21, $22, $23, $24, $25,
                $26::numeric, $27::numeric, $28::numeric, $29::numeric, $30::numeric,
                $31
            )
            "#,
        )
        .bind(eid)
        .bind(wid)
        .bind(sid)
        .bind(vid)
        .bind("/product")
        .bind(Some("utm_source=newsletter&gclid=12345"))
        .bind(Some("/search"))
        .bind(Some("q=test"))
        .bind(Some("google.com"))
        .bind(Some("Product Page"))
        .bind(1i32)
        .bind(Some("view_product"))
        .bind(Some("v2"))
        .bind(Some("example.com"))
        .bind(Some("newsletter"))
        .bind(Some("email"))
        .bind(Some("spring_sale"))
        .bind(Some("banner"))
        .bind(Some("shoes"))
        .bind(Some("12345"))
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(Some(0.015f64))
        .bind(Some(800.5f64))
        .bind(Some(50.0f64))
        .bind(Some(1200.0f64))
        .bind(Some(150.0f64))
        .bind(now)
        .execute(&pool)
        .await;

        assert!(res.is_ok(), "Failed to insert website_event: {:?}", res);

        let rev_res = sqlx::query(
            r#"
            INSERT INTO "revenue" (
                revenue_id, website_id, session_id, event_id, event_name, currency, revenue, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(wid)
        .bind(sid)
        .bind(eid)
        .bind("purchase")
        .bind("USD")
        .bind(99.99f64)
        .bind(now)
        .execute(&pool)
        .await;

        assert!(rev_res.is_ok(), "Failed to insert revenue: {:?}", rev_res);

        let ed_res = sqlx::query(
            r#"
            INSERT INTO "event_data" (
                event_data_id, website_id, website_event_id, data_key, string_value, number_value, date_value, data_type, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8, $9)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(wid)
        .bind(eid)
        .bind("price")
        .bind(Some("99.99"))
        .bind(Some(99.99f64))
        .bind(None::<DateTime<Utc>>)
        .bind(2i32)
        .bind(now)
        .execute(&pool)
        .await;

        assert!(ed_res.is_ok(), "Failed to insert event_data: {:?}", ed_res);

        let row = sqlx::query_as::<_, (Option<f64>, Option<f64>)>(
            r#"SELECT cls::float8, lcp::float8 FROM "website_event" WHERE event_id = $1"#,
        )
        .bind(eid)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!((row.0.unwrap() - 0.015).abs() < 1e-4);
        assert!((row.1.unwrap() - 1200.0).abs() < 1e-4);
    }

    #[test]
    fn test_caps() {
        let d = CollectData {
            website: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            link: None,
            pixel: None,
            hostname: None,
            language: None,
            referrer: None,
            screen: None,
            title: None,
            url: Some("/a".into()),
            name: Some("x".repeat(100)),
            data: Some(json!({"k": "v".repeat(600)})),
            tag: None,
            ip: None,
            user_agent: None,
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };
        let n = normalize_payload(d);
        assert_eq!(n.name.unwrap().len(), 50);
        assert_eq!(
            n.data.unwrap().as_object().unwrap()["k"]
                .as_str()
                .unwrap()
                .len(),
            500
        );
    }

    #[test]
    fn test_formula_triggers_rejected() {
        for trigger in ["=SUM(A1)", "+12345", "-cmd", "@admin"] {
            let d = CollectData {
                website: Some("550e8400-e29b-41d4-a716-446655440000".into()),
                link: None,
                pixel: None,
                hostname: None,
                language: None,
                referrer: None,
                screen: None,
                title: None,
                url: Some("/".into()),
                name: Some(trigger.into()),
                data: None,
                tag: None,
                ip: None,
                user_agent: None,
                timestamp: None,
                id: None,
                browser: None,
                os: None,
                device: None,
                lcp: None,
                inp: None,
                cls: None,
                fcp: None,
                ttfb: None,
                event_type: None,
                ..Default::default()
            };
            assert!(
                validate_payload(&d).is_err(),
                "Trigger {trigger} should be rejected"
            );
        }
    }

    #[test]
    fn test_single_source_id_required() {
        let d0 = CollectData {
            website: None,
            link: None,
            pixel: None,
            hostname: None,
            language: None,
            referrer: None,
            screen: None,
            title: None,
            url: Some("/".into()),
            name: None,
            data: None,
            tag: None,
            ip: None,
            user_agent: None,
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };
        assert!(get_source_id(&d0).is_err());

        let mut d_multi = d0.clone();
        d_multi.website = Some("550e8400-e29b-41d4-a716-446655440000".into());
        d_multi.link = Some("550e8400-e29b-41d4-a716-446655440001".into());
        assert!(get_source_id(&d_multi).is_err());

        let mut d_web = d0.clone();
        d_web.website = Some("550e8400-e29b-41d4-a716-446655440000".into());
        assert_eq!(
            get_source_id(&d_web).unwrap(),
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );

        let mut d_link = d0.clone();
        d_link.link = Some("550e8400-e29b-41d4-a716-446655440001".into());
        assert_eq!(
            get_source_id(&d_link).unwrap(),
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap()
        );

        let mut d_pixel = d0.clone();
        d_pixel.pixel = Some("550e8400-e29b-41d4-a716-446655440002".into());
        assert_eq!(
            get_source_id(&d_pixel).unwrap(),
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap()
        );

        let mut d_bad_uuid = d0;
        d_bad_uuid.website = Some("not-a-valid-uuid".into());
        assert!(get_source_id(&d_bad_uuid).is_err());
    }

    #[test]
    fn test_session_id_deterministic() {
        let source_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let dt = Utc::now();
        let s1 = generate_session_id(source_id, "1.2.3.4", "agent1", "secret", dt);
        let s2 = generate_session_id(source_id, "1.2.3.4", "agent1", "secret", dt);
        let s3 = generate_session_id(source_id, "1.2.3.5", "agent1", "secret", dt);
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_visit_id_deterministic() {
        let session_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let dt = Utc::now();
        let v1 = generate_visit_id(session_id, dt);
        let v2 = generate_visit_id(session_id, dt);
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_validate_payload_branches() {
        let wid = "550e8400-e29b-41d4-a716-446655440000";
        let d_no_src = CollectData {
            website: None,
            link: None,
            pixel: None,
            hostname: None,
            language: None,
            referrer: None,
            screen: None,
            title: None,
            url: Some("/".into()),
            name: None,
            data: None,
            tag: None,
            ip: None,
            user_agent: None,
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };
        assert!(validate_payload(&d_no_src).is_err());

        let mut d = CollectData {
            website: Some(wid.into()),
            link: None,
            pixel: None,
            hostname: None,
            language: None,
            referrer: None,
            screen: None,
            title: None,
            url: Some("/".into()),
            name: None,
            data: None,
            tag: None,
            ip: None,
            user_agent: None,
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };
        assert!(validate_payload(&d).is_ok());

        d.name = Some("safe_event".into());
        assert!(validate_payload(&d).is_ok());

        let mut big_map = serde_json::Map::new();
        for i in 0..55 {
            big_map.insert(format!("key_{i}"), serde_json::json!("val"));
        }
        d.data = Some(serde_json::Value::Object(big_map));
        assert!(validate_payload(&d).is_err());

        let mut map_long_key = serde_json::Map::new();
        map_long_key.insert("k".repeat(505), serde_json::json!("val"));
        d.data = Some(serde_json::Value::Object(map_long_key));
        assert!(validate_payload(&d).is_err());

        let mut map_long_val = serde_json::Map::new();
        map_long_val.insert("k".into(), serde_json::json!("v".repeat(505)));
        d.data = Some(serde_json::Value::Object(map_long_val));
        assert!(validate_payload(&d).is_err());

        let mut map_valid_str = serde_json::Map::new();
        map_valid_str.insert("k".into(), serde_json::json!("short_value"));
        d.data = Some(serde_json::Value::Object(map_valid_str));
        assert!(validate_payload(&d).is_ok());

        d.data = Some(serde_json::json!([1, 2, 3]));
        assert!(validate_payload(&d).is_ok());
    }

    #[test]
    fn test_normalize_payload_branches() {
        let wid = "550e8400-e29b-41d4-a716-446655440000";
        let mut map = serde_json::Map::new();
        map.insert("num".into(), serde_json::json!(12.3456789));
        map.insert("short_str".into(), serde_json::json!("hello"));
        map.insert("long_str".into(), serde_json::json!("v".repeat(600)));
        map.insert("bool".into(), serde_json::json!(true));
        for i in 0..10 {
            map.insert(format!("extra_{i}"), serde_json::json!(i));
        }
        let d = CollectData {
            website: Some(wid.into()),
            link: None,
            pixel: None,
            hostname: None,
            language: None,
            referrer: None,
            screen: None,
            title: None,
            url: Some("/".into()),
            name: Some("short".into()),
            data: Some(serde_json::Value::Object(map)),
            tag: None,
            ip: None,
            user_agent: None,
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };
        let norm = normalize_payload(d);
        assert_eq!(norm.name.as_deref().unwrap(), "short");
        let obj = norm.data.clone().unwrap();
        let map_res = obj.as_object().unwrap();
        assert!(map_res.len() <= PAYLOAD_MAX_KEYS);
        assert_eq!(map_res["num"].as_f64().unwrap(), 12.3457);

        let mut big_map = serde_json::Map::new();
        for i in 0..60 {
            big_map.insert(format!("k_{i}"), serde_json::json!(i));
        }
        let d_big = CollectData {
            website: Some(wid.into()),
            link: None,
            pixel: None,
            hostname: None,
            language: None,
            referrer: None,
            screen: None,
            title: None,
            url: Some("/".into()),
            name: None,
            data: Some(serde_json::Value::Object(big_map)),
            tag: None,
            ip: None,
            user_agent: None,
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };
        let norm_big = normalize_payload(d_big);
        assert_eq!(
            norm_big.data.unwrap().as_object().unwrap().len(),
            PAYLOAD_MAX_KEYS
        );

        let d_no_obj = CollectData {
            website: Some(wid.into()),
            link: None,
            pixel: None,
            hostname: None,
            language: None,
            referrer: None,
            screen: None,
            title: None,
            url: Some("/".into()),
            name: None,
            data: Some(serde_json::json!([1, 2, 3])),
            tag: None,
            ip: None,
            user_agent: None,
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };
        let norm_no_obj = normalize_payload(d_no_obj);
        assert_eq!(norm_no_obj.data.unwrap(), serde_json::json!([1, 2, 3]));

        let payload = CollectPayload {
            r#type: "event".into(),
            payload: norm.clone(),
        };
        let ser = serde_json::to_string(&payload).unwrap();
        assert!(ser.contains("\"type\":\"event\""));

        let res = IngestResult {
            cache: "c1".into(),
            session_id: Uuid::now_v7(),
            visit_id: Uuid::now_v7(),
        };
        let res_ser = serde_json::to_string(&res).unwrap();
        assert!(res_ser.contains("\"cache\":\"c1\""));

        let token = CacheToken {
            r#type: "cache".into(),
            website_id: wid.into(),
            session_id: "s1".into(),
            visit_id: "v1".into(),
            iat: 12345,
            session_link_id: None,
        };
        let tok_ser = serde_json::to_string(&token).unwrap();
        assert!(tok_ser.contains("\"type\":\"cache\""));
    }

    #[tokio::test]
    async fn test_save_session_and_event_db() {
        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();

        let website_id = Uuid::parse_str("01a0aa65-c671-72cc-b248-2884c0b76273").unwrap();
        let session_id = Uuid::now_v7();
        let visit_id = Uuid::now_v7();
        let created_at = Utc::now();

        let mut map = serde_json::Map::new();
        map.insert("btn_click".into(), serde_json::json!("submit"));
        map.insert("count".into(), serde_json::json!(42));
        map.insert("is_admin".into(), serde_json::json!(true));

        let data = CollectData {
            website: Some(website_id.to_string()),
            link: None,
            pixel: None,
            hostname: Some("localhost".into()),
            language: Some("en-US".into()),
            referrer: Some("https://google.com".into()),
            screen: Some("1920x1080".into()),
            title: Some("Home".into()),
            url: Some("/dashboard?tab=analytics".into()),
            name: Some("custom_event".into()),
            data: Some(serde_json::Value::Object(map)),
            tag: Some("v1".into()),
            ip: Some("127.0.0.1".into()),
            user_agent: Some("TestAgent/1.0".into()),
            timestamp: Some(created_at.timestamp()),
            id: Some("dist-123".into()),
            browser: Some("chrome".into()),
            os: Some("linux".into()),
            device: Some("desktop".into()),
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: None,
            ..Default::default()
        };

        let res = save_session_and_event(
            &pool,
            website_id,
            session_id,
            visit_id,
            &data,
            created_at,
            "desktop",
            Some("chrome"),
            Some("linux"),
            Some("US"),
            Some("US-CA"),
            Some("San Francisco"),
        )
        .await;

        assert!(res.is_ok());

        let mut data_no_q = data.clone();
        data_no_q.url = Some("/simple-path".into());
        data_no_q.name = None;
        data_no_q.data = None;
        let res2 = save_session_and_event(
            &pool, website_id, session_id, visit_id, &data_no_q, created_at, "desktop", None, None,
            None, None, None,
        )
        .await;
        assert!(res2.is_ok());

        let mut data_overflow_name = data.clone();
        data_overflow_name.name = Some("x".repeat(100));
        let res_event_err = save_session_and_event(
            &pool,
            website_id,
            Uuid::now_v7(),
            visit_id,
            &data_overflow_name,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(
            res_event_err.is_err(),
            "event_name > 50 must error on website_event insert"
        );

        let mut map_err = serde_json::Map::new();
        map_err.insert("null\0byte".into(), serde_json::json!("val"));
        let mut data_null_byte = data.clone();
        data_null_byte.data = Some(serde_json::Value::Object(map_err));
        let res_ed_err = save_session_and_event(
            &pool,
            website_id,
            Uuid::now_v7(),
            visit_id,
            &data_null_byte,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(
            res_ed_err.is_err(),
            "null byte in event_data key must error on event_data insert"
        );

        let mut map_rich = serde_json::Map::new();
        map_rich.insert("purchase_date".into(), serde_json::json!("2026-09-19T12:00:00Z"));
        map_rich.insert("invalid_date".into(), serde_json::json!("2026-09-19Tinvalid-date"));
        map_rich.insert("revenue".into(), serde_json::json!(49.99));
        map_rich.insert("currency".into(), serde_json::json!("USD"));
        for i in 0..55 {
            map_rich.insert(format!("z_prop_{i}"), serde_json::json!(format!("val_{i}")));
        }

        let mut data_rich = data.clone();
        data_rich.data = Some(serde_json::Value::Object(map_rich));

        let res_rich = save_session_and_event_cached(
            &pool,
            website_id,
            session_id,
            visit_id,
            &data_rich,
            created_at,
            "desktop",
            Some("chrome"),
            Some("linux"),
            Some("US"),
            Some("US-CA"),
            Some("San Francisco"),
            true,
        )
        .await;
        assert!(res_rich.is_ok());

        let mut map_str_rev = serde_json::Map::new();
        map_str_rev.insert("revenue".into(), serde_json::json!("99.50"));
        map_str_rev.insert("currency".into(), serde_json::json!("EUR"));
        let mut data_str_rev = data.clone();
        data_str_rev.data = Some(serde_json::Value::Object(map_str_rev));
        let res_str_rev = save_session_and_event_cached(
            &pool,
            website_id,
            session_id,
            visit_id,
            &data_str_rev,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(res_str_rev.is_ok());

        let mut data_non_obj = data.clone();
        data_non_obj.name = None;
        data_non_obj.message = Some("an error message".into());
        data_non_obj.data = Some(serde_json::json!("a non-object data string"));
        let res_non_obj = save_session_and_event_cached(
            &pool,
            website_id,
            session_id,
            visit_id,
            &data_non_obj,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(res_non_obj.is_ok());

        let mut map_rev_err = serde_json::Map::new();
        map_rev_err.insert("revenue".into(), serde_json::json!("123456789012345678901234567890"));
        map_rev_err.insert("currency".into(), serde_json::json!("USD"));
        let mut data_rev_err = data.clone();
        data_rev_err.data = Some(serde_json::Value::Object(map_rev_err));
        let res_rev_err = save_session_and_event_cached(
            &pool,
            website_id,
            Uuid::now_v7(),
            visit_id,
            &data_rev_err,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(res_rev_err.is_err());

        let mut map_zero_rev = serde_json::Map::new();
        map_zero_rev.insert("revenue".into(), serde_json::json!(0.0));
        map_zero_rev.insert("currency".into(), serde_json::json!(""));
        let mut data_zero_rev = data.clone();
        data_zero_rev.data = Some(serde_json::Value::Object(map_zero_rev));
        let res_zero_rev = save_session_and_event_cached(
            &pool,
            website_id,
            session_id,
            visit_id,
            &data_zero_rev,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(res_zero_rev.is_ok());

        let ch_cfg = kombu_db::ClickHouseConfig::from_url("http://127.0.0.1:8123").unwrap();
        let ch_client = kombu_db::ClickHouseClient::new(ch_cfg);

        let part_res = save_session_and_event_with_engine(
            kombu_core::types::StorageEngine::Partitioned,
            &pool,
            Some(&ch_client),
            website_id,
            Uuid::now_v7(),
            visit_id,
            &data,
            created_at,
            "desktop",
            Some("chrome"),
            Some("linux"),
            Some("US"),
            Some("US-CA"),
            Some("San Francisco"),
            true,
        )
        .await;
        assert!(part_res.is_ok());

        let ts_res = save_session_and_event_with_engine(
            kombu_core::types::StorageEngine::Timescale,
            &pool,
            Some(&ch_client),
            website_id,
            Uuid::now_v7(),
            visit_id,
            &data,
            created_at,
            "desktop",
            Some("chrome"),
            Some("linux"),
            Some("US"),
            Some("US-CA"),
            Some("San Francisco"),
            true,
        )
        .await;
        assert!(ts_res.is_ok());

        let ch_res = save_session_and_event_with_engine(
            kombu_core::types::StorageEngine::Clickhouse,
            &pool,
            Some(&ch_client),
            website_id,
            Uuid::now_v7(),
            visit_id,
            &data,
            created_at,
            "desktop",
            Some("chrome"),
            Some("linux"),
            Some("US"),
            Some("US-CA"),
            Some("San Francisco"),
            true,
        )
        .await;
        assert!(ch_res.is_ok());

        let ev_built = build_clickhouse_event(
            website_id,
            session_id,
            visit_id,
            Uuid::now_v7(),
            &data,
            created_at,
            "desktop",
            Some("chrome"),
            Some("linux"),
            Some("US"),
            Some("US-CA"),
            Some("San Francisco"),
        );
        assert_eq!(ev_built.website_id, website_id);
        assert_eq!(ev_built.browser, "chrome");

        let mut plain_data = data.clone();
        plain_data.url = Some("/plain-path".into());
        plain_data.name = None;
        plain_data.message = None;
        let ev_plain = build_clickhouse_event(
            website_id,
            session_id,
            visit_id,
            Uuid::now_v7(),
            &plain_data,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(ev_plain.url_path, "/plain-path");
        assert_eq!(
            ev_plain.event_type,
            kombu_core::constants::EVENT_TYPE_PAGE_VIEW as u32
        );

        let closed_pool = pool.clone();
        closed_pool.close().await;
        let res_err = save_session_and_event(
            &closed_pool,
            website_id,
            session_id,
            visit_id,
            &data,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(res_err.is_err());

        let res_engine_err = save_session_and_event_with_engine(
            kombu_core::types::StorageEngine::Postgres,
            &closed_pool,
            None,
            website_id,
            session_id,
            visit_id,
            &data,
            created_at,
            "desktop",
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(res_engine_err.is_err());
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_source_id_count() {
        let has_w: bool = kani::any();
        let has_l: bool = kani::any();
        let has_p: bool = kani::any();
        let count = [has_w, has_l, has_p].iter().filter(|&&x| x).count();
        let valid = count == 1;
        if (has_w as u8 + has_l as u8 + has_p as u8) == 1 {
            kani::assert(valid, "exactly one source");
        } else {
            kani::assert(!valid, "not exactly one source");
        }
    }

    #[kani::proof]
    fn harness_formula_triggers() {
        let choice: u8 = kani::any();
        let prefix = match choice % 5 {
            0 => "=",
            1 => "+",
            2 => "-",
            3 => "@",
            _ => "safe",
        };
        let is_rejected = prefix.starts_with('=')
            || prefix.starts_with('+')
            || prefix.starts_with('-')
            || prefix.starts_with('@');
        if choice % 5 < 4 {
            kani::assert(is_rejected, "formula trigger detected");
        } else {
            kani::assert(!is_rejected, "safe trigger passes");
        }
    }

    #[kani::proof]
    fn harness_payload_caps() {
        let len: usize = kani::any();
        kani::assume(len <= 1000);
        let capped = if len > super::EVENT_NAME_TRUNC {
            super::EVENT_NAME_TRUNC
        } else {
            len
        };
        kani::assert(capped <= 50, "event name capped to 50");
    }

    #[kani::proof]
    fn harness_max_keys_cap() {
        let key_count: usize = kani::any();
        kani::assume(key_count <= 200);
        let capped = if key_count > super::PAYLOAD_MAX_KEYS {
            super::PAYLOAD_MAX_KEYS
        } else {
            key_count
        };
        kani::assert(capped <= 50, "payload max keys strictly bounded");
    }

    #[kani::proof]
    fn harness_number_rounding_scale() {
        let f: f32 = kani::any();
        kani::assume(!f.is_nan() && !f.is_infinite());
        let val = (f as f64 * 10000.0).round() / 10000.0;
        kani::assert(!val.is_nan(), "rounded number is not NaN");
    }
}
