#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::mpsc::{Sender, channel};
use uuid::Uuid;

use kombu_core::constants::{DATA_TYPE_DATE, DATA_TYPE_NUMBER, EVENT_DATA_MAX_KEYS};
use kombu_core::data::{flatten_event_data, flatten_json};
use kombu_core::types::determine_event_type;
use kombu_core::url::{
    parse_query_params, parse_referrer, truncate_event_name, truncate_string, truncate_url_path,
};
use kombu_ingest::CollectData;

const KEY_REVENUE: &str = "revenue";
const KEY_AMOUNT: &str = "amount";
const KEY_CURRENCY: &str = "currency";
const DEFAULT_CURRENCY: &str = "USD";
const DEFAULT_REVENUE_EVENT: &str = "Purchase";

#[derive(Clone)]
pub struct IngestItem {
    pub source_id: Uuid,
    pub session_id: Uuid,
    pub visit_id: Uuid,
    pub data: CollectData,
    pub created_at: DateTime<Utc>,
    pub device: String,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub session_known_exists: bool,
    pub is_bot: bool,
    pub bot_score: u8,
}

#[derive(Clone)]
pub struct IngestQueue {
    pub(crate) senders: Vec<Sender<IngestItem>>,
    pub(crate) mask: usize,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum QueueError {
    #[error("Ingest queue is full")]
    Full,
    #[error("Ingest queue is closed")]
    Closed,
}

impl IngestQueue {
    pub fn new(pool: &PgPool, capacity: usize, num_shards: usize) -> Self {
        let num_shards = num_shards.next_power_of_two();
        let mask = num_shards - 1;
        let shard_cap = capacity / num_shards;
        let mut senders = Vec::with_capacity(num_shards);

        for _ in 0..num_shards {
            let (tx, mut rx) = channel::<IngestItem>(shard_cap);
            senders.push(tx);
            let pool = pool.clone();
            tokio::spawn(async move {
                const MAX_BATCH_SIZE: usize = 5_000;
                let mut batch: Vec<IngestItem> = Vec::with_capacity(MAX_BATCH_SIZE);

                loop {
                    batch.clear();
                    match rx.recv().await {
                        Some(item) => batch.push(item),
                        None => break,
                    }
                    while batch.len() < MAX_BATCH_SIZE {
                        match rx.try_recv() {
                            Ok(item) => batch.push(item),
                            Err(_) => break,
                        }
                    }

                    let Ok(mut tx) = pool.begin().await else {
                        continue;
                    };
                    for item in &batch {
                            if !item.session_known_exists {
                                let _ = sqlx::query(
                                    r#"
                                    INSERT INTO "session" (
                                        session_id, website_id, browser, os, device, screen, language, country, region, city, distinct_id, is_bot, created_at
                                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                                    ON CONFLICT (session_id) DO NOTHING
                                    "#,
                                )
                                .bind(item.session_id)
                                .bind(item.source_id)
                                .bind(item.browser.as_deref())
                                .bind(item.os.as_deref())
                                .bind(&item.device)
                                .bind(&item.data.screen)
                                .bind(&item.data.language)
                                .bind(item.country.as_deref())
                                .bind(item.region.as_deref())
                                .bind(item.city.as_deref())
                                .bind(&item.data.id)
                                .bind(item.is_bot)
                                .bind(item.created_at)
                                .execute(&mut *tx)
                                .await;
                            }

                            let event_id = Uuid::now_v7();
                            let url_raw = item.data.url.as_deref().unwrap_or("/");
                            let (raw_path, raw_query) = match url_raw.split_once('?') {
                                Some((p, q)) => (p, Some(q)),
                                None => (url_raw, None),
                            };
                            let url_path = truncate_url_path(raw_path);
                            let url_query = raw_query.map(|q| {
                                let sanitized = kombu_core::url::sanitize_query_string(q);
                                truncate_string(&sanitized, 500)
                            });

                            let query_params =
                                raw_query.map(parse_query_params).unwrap_or_default();

                            let parsed_ref = item
                                .data
                                .referrer
                                .as_deref()
                                .map(|r| parse_referrer(r, item.data.hostname.as_deref()))
                                .unwrap_or_default();

                            let is_error = item.data.event_type
                                == Some(kombu_core::constants::EVENT_TYPE_ERROR)
                                || item.data.message.is_some()
                                || item.data.stack.is_some();

                            let event_type = item.data.event_type.unwrap_or_else(|| {
                                determine_event_type(
                                    item.data.link.is_some(),
                                    item.data.pixel.is_some(),
                                    item.data.lcp.is_some()
                                        || item.data.inp.is_some()
                                        || item.data.cls.is_some()
                                        || item.data.fcp.is_some()
                                        || item.data.ttfb.is_some(),
                                    is_error,
                                    item.data.name.is_some() || item.data.message.is_some(),
                                )
                            });

                            let event_title =
                                item.data.title.as_deref().map(|s| truncate_string(s, 500));
                            let event_name =
                                item.data.name.as_deref().or(item.data.message.as_deref());
                            let tag = item.data.tag.as_deref().map(|s| truncate_string(s, 50));
                            let hostname = item
                                .data
                                .hostname
                                .as_deref()
                                .map(|s| truncate_string(s, 100));

                            let _ = sqlx::query(
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
                            .bind(item.source_id)
                            .bind(item.session_id)
                            .bind(item.visit_id)
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
                            .bind(item.data.cls)
                            .bind(item.data.fcp)
                            .bind(item.data.inp)
                            .bind(item.data.lcp)
                            .bind(item.data.ttfb)
                            .bind(item.is_bot)
                            .bind(i16::from(item.bot_score))
                            .bind(item.created_at)
                            .execute(&mut *tx)
                            .await;

                            if let Some(event_data_val) = &item.data.data {
                                let flattened =
                                    flatten_event_data(event_data_val).unwrap_or_else(|_| {
                                        let mut out = Vec::new();
                                        flatten_json(event_data_val, "", &mut out);
                                        out.truncate(EVENT_DATA_MAX_KEYS);
                                        out
                                    });

                                for item_data in flattened {
                                    let number_val = if item_data.data_type == DATA_TYPE_NUMBER {
                                        item_data.value.parse::<f64>().ok()
                                    } else {
                                        None
                                    };
                                    let date_val = if item_data.data_type == DATA_TYPE_DATE {
                                        DateTime::parse_from_rfc3339(&item_data.value)
                                            .ok()
                                            .map(|dt| dt.with_timezone(&Utc))
                                    } else {
                                        None
                                    };
                                    let truncated_key = truncate_string(&item_data.key, 500);

                                    let _ = sqlx::query(
                                        r#"
                                        INSERT INTO "event_data" (
                                            event_data_id, website_id, website_event_id, data_key, string_value, number_value, date_value, data_type, created_at
                                        ) VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8, $9)
                                        "#,
                                    )
                                    .bind(Uuid::now_v7())
                                    .bind(item.source_id)
                                    .bind(event_id)
                                    .bind(truncated_key)
                                    .bind(Some(item_data.value))
                                    .bind(number_val)
                                    .bind(date_val)
                                    .bind(item_data.data_type)
                                    .bind(item.created_at)
                                    .execute(&mut *tx)
                                    .await;
                                }

                                if let Some(map) = event_data_val.as_object() {
                                    let revenue_val = map
                                        .get(KEY_REVENUE)
                                        .or_else(|| map.get(KEY_AMOUNT))
                                        .and_then(|v| {
                                            v.as_f64().or_else(|| {
                                                v.as_str().and_then(|s| s.parse::<f64>().ok())
                                            })
                                        })
                                        .filter(|&rev| rev > 0.0);

                                    if let Some(rev) = revenue_val {
                                        let rev_id = Uuid::now_v7();
                                        let ev_name = item
                                            .data
                                            .name
                                            .as_deref()
                                            .filter(|s| !s.is_empty())
                                            .unwrap_or(DEFAULT_REVENUE_EVENT);
                                        let truncated_ev_name = truncate_event_name(ev_name);
                                        let currency_val = map
                                            .get(KEY_CURRENCY)
                                            .and_then(|v| v.as_str())
                                            .map(str::trim)
                                            .filter(|s| !s.is_empty())
                                            .unwrap_or(DEFAULT_CURRENCY);
                                        let truncated_currency =
                                            truncate_string(currency_val, 10);

                                        let _ = sqlx::query(
                                            r#"
                                            INSERT INTO "revenue" (
                                                revenue_id, website_id, session_id, event_id, event_name, currency, revenue, created_at
                                            ) VALUES ($1, $2, $3, $4, $5, $6, $7::numeric, $8)
                                            "#,
                                        )
                                        .bind(rev_id)
                                        .bind(item.source_id)
                                        .bind(item.session_id)
                                        .bind(event_id)
                                        .bind(truncated_ev_name)
                                        .bind(truncated_currency)
                                        .bind(rev)
                                        .bind(item.created_at)
                                        .execute(&mut *tx)
                                        .await;
                                    }
                                }
                            }
                        }
                        let _ = tx.commit().await;
                }
            });
        }

        Self { senders, mask }
    }

    #[inline]
    fn shard_index(&self, session_id: &uuid::Uuid) -> usize {
        (session_id.as_u128() as usize) & self.mask
    }

    #[inline]
    pub fn try_push(&self, item: IngestItem) -> Result<(), QueueError> {
        let idx = self.shard_index(&item.session_id);
        match self.senders[idx].try_send(item) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => Err(QueueError::Full),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => Err(QueueError::Closed),
        }
    }

    pub async fn push(&self, item: IngestItem) -> Result<(), QueueError> {
        let idx = self.shard_index(&item.session_id);
        match self.senders[idx].try_send(item) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(it)) => self.senders[idx]
                .send(it)
                .await
                .map_err(|_| QueueError::Closed),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => Err(QueueError::Closed),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_error_display() {
        assert_eq!(QueueError::Full.to_string(), "Ingest queue is full");
        assert_eq!(QueueError::Closed.to_string(), "Ingest queue is closed");
    }

    #[tokio::test]
    async fn test_queue_lifecycle_and_db_flush() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

        let website_id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO website (website_id, name, domain, created_at, updated_at) VALUES ($1, 'QueueTest', 'queue.test', NOW(), NOW())"#,
        )
        .bind(website_id)
        .execute(&pool)
        .await
        .unwrap();

        let queue = IngestQueue::new(&pool, 16, 2);

        let session_id_1 = Uuid::now_v7();
        let visit_id_1 = Uuid::now_v7();
        let created_at = Utc::now();
        let item1 = IngestItem {
            source_id: website_id,
            session_id: session_id_1,
            visit_id: visit_id_1,
            data: CollectData {
                website: Some(website_id.to_string()),
                link: None,
                pixel: None,
                hostname: Some("queue.test".into()),
                language: Some("en-US".into()),
                referrer: Some("https://google.com/search?q=rust".into()),
                screen: Some("1920x1080".into()),
                title: Some("Queue Page".into()),
                url: Some("/products/item?utm_source=ad&gclid=123".into()),
                name: Some("checkout".into()),
                data: Some(serde_json::json!({
                    "revenue": 49.99,
                    "currency": "EUR",
                    "category": "shoes",
                    "quantity": 2,
                    "released_at": "2026-09-19T00:00:00Z"
                })),
                tag: Some("sale".into()),
                ip: Some("127.0.0.1".into()),
                user_agent: Some("Mozilla/5.0 Chrome".into()),
                timestamp: None,
                id: Some("distinct_123".into()),
                browser: Some("Chrome".into()),
                os: Some("Linux".into()),
                device: Some("desktop".into()),
                lcp: Some(1.2),
                inp: Some(50.0),
                cls: Some(0.01),
                fcp: Some(0.8),
                ttfb: Some(0.1),
                event_type: Some(2),
                is_bot: Some(false),
                bot_score: Some(0),
                ..Default::default()
            },
            created_at,
            device: "desktop".into(),
            browser: Some("Chrome".into()),
            os: Some("Linux".into()),
            country: Some("US".into()),
            region: Some("CA".into()),
            city: Some("San Francisco".into()),
            session_known_exists: false,
            is_bot: false,
            bot_score: 0,
        };

        queue.push(item1.clone()).await.unwrap();

        let session_id_2 = session_id_1;
        let visit_id_2 = Uuid::now_v7();
        let mut big_data = serde_json::Map::new();
        big_data.insert("amount".into(), serde_json::json!("19.50"));
        for i in 0..55 {
            big_data.insert(format!("prop_{i}"), serde_json::json!(format!("val_{i}")));
        }

        let item2 = IngestItem {
            source_id: website_id,
            session_id: session_id_2,
            visit_id: visit_id_2,
            data: CollectData {
                website: Some(website_id.to_string()),
                link: Some("link_1".into()),
                pixel: None,
                hostname: None,
                language: None,
                referrer: None,
                screen: None,
                title: None,
                url: Some("/simple_path".into()),
                name: None,
                data: Some(serde_json::Value::Object(big_data)),
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
                is_bot: None,
                bot_score: None,
                ..Default::default()
            },
            created_at,
            device: "mobile".into(),
            browser: None,
            os: None,
            country: None,
            region: None,
            city: None,
            session_known_exists: true,
            is_bot: false,
            bot_score: 0,
        };

        queue.try_push(item2).unwrap();

        let item3 = IngestItem {
            source_id: website_id,
            session_id: session_id_1,
            visit_id: Uuid::now_v7(),
            data: CollectData {
                website: Some(website_id.to_string()),
                data: Some(serde_json::json!({ "revenue": 0.0, "currency": "EUR" })),
                url: Some("/no-rev".into()),
                ..Default::default()
            },
            created_at,
            device: "desktop".into(),
            browser: None,
            os: None,
            country: None,
            region: None,
            city: None,
            session_known_exists: true,
            is_bot: false,
            bot_score: 0,
        };
        queue.try_push(item3).unwrap();

        let item_str = IngestItem {
            source_id: website_id,
            session_id: session_id_1,
            visit_id: Uuid::now_v7(),
            data: CollectData {
                website: Some(website_id.to_string()),
                data: Some(serde_json::Value::String("not_obj".into())),
                ..Default::default()
            },
            created_at,
            device: "desktop".into(),
            browser: None,
            os: None,
            country: None,
            region: None,
            city: None,
            session_known_exists: true,
            is_bot: false,
            bot_score: 0,
        };
        queue.try_push(item_str).unwrap();

        let item_no_data = IngestItem {
            source_id: website_id,
            session_id: session_id_1,
            visit_id: Uuid::now_v7(),
            data: CollectData {
                website: Some(website_id.to_string()),
                data: None,
                ..Default::default()
            },
            created_at,
            device: "desktop".into(),
            browser: None,
            os: None,
            country: None,
            region: None,
            city: None,
            session_known_exists: true,
            is_bot: false,
            bot_score: 0,
        };
        queue.try_push(item_no_data).unwrap();

        let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        closed_pool.close().await;
        let queue_fail = IngestQueue::new(&closed_pool, 1, 1);
        let _ = queue_fail.try_push(item1.clone());

        for _ in 0..20 {
            let count: i64 =
                sqlx::query_scalar(r#"SELECT COUNT(*) FROM "session" WHERE session_id = $1"#)
                    .bind(session_id_1)
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(0);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if count >= 1 {
                break;
            }
        }

        let session_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "session" WHERE session_id = $1"#)
                .bind(session_id_1)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(session_count, 1);

        let event_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "website_event" WHERE website_id = $1"#)
                .bind(website_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(event_count >= 2);

        let rev_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "revenue" WHERE website_id = $1"#)
                .bind(website_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(rev_count >= 1);

        let _ = sqlx::query(r#"DELETE FROM "revenue" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "event_data" WHERE website_id = $1"#)
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

        drop(queue);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_queue_full_and_closed_errors() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<IngestItem>(1);
        let queue = IngestQueue {
            senders: vec![tx],
            mask: 0,
        };

        let dummy_item = IngestItem {
            source_id: Uuid::nil(),
            session_id: Uuid::nil(),
            visit_id: Uuid::nil(),
            data: CollectData::default(),
            created_at: Utc::now(),
            device: "desktop".into(),
            browser: None,
            os: None,
            country: None,
            region: None,
            city: None,
            session_known_exists: false,
            is_bot: false,
            bot_score: 0,
        };

        queue.try_push(dummy_item.clone()).unwrap();

        let err_full = queue.try_push(dummy_item.clone());
        assert_eq!(err_full, Err(QueueError::Full));

        let rx_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let _ = rx.recv().await;
            rx
        });
        let res_push = queue.push(dummy_item.clone()).await;
        assert!(res_push.is_ok());

        let rx = rx_task.await.unwrap();

        let rx_drop_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            drop(rx);
        });
        let res_push_closed = queue.push(dummy_item.clone()).await;
        assert_eq!(res_push_closed, Err(QueueError::Closed));
        let _ = rx_drop_task.await;

        let err_closed = queue.try_push(dummy_item.clone());
        assert_eq!(err_closed, Err(QueueError::Closed));

        let err_push_closed_direct = queue.push(dummy_item).await;
        assert_eq!(err_push_closed_direct, Err(QueueError::Closed));
    }
}
