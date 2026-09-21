#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WebsiteId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VisitId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteEvent {
    pub id: EventId,
    pub website_id: WebsiteId,
    pub session_id: SessionId,
    pub visit_id: VisitId,
    pub url_path: String,
    pub url_query: Option<String>,
    pub referrer_path: Option<String>,
    pub referrer_query: Option<String>,
    pub referrer_domain: Option<String>,
    pub page_title: Option<String>,
    pub event_type: i32,
    pub event_name: Option<String>,
    pub tag: Option<String>,
    pub hostname: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_content: Option<String>,
    pub utm_term: Option<String>,
    pub gclid: Option<String>,
    pub fbclid: Option<String>,
    pub msclkid: Option<String>,
    pub ttclid: Option<String>,
    pub li_fat_id: Option<String>,
    pub twclid: Option<String>,
    pub cls: Option<f64>,
    pub fcp: Option<f64>,
    pub inp: Option<f64>,
    pub lcp: Option<f64>,
    pub ttfb: Option<f64>,
    pub ttl: Option<i64>,
    pub is_bot: bool,
}

impl WebsiteEvent {
    pub fn new(
        website_id: WebsiteId,
        session_id: SessionId,
        visit_id: VisitId,
        url_path: impl Into<String>,
    ) -> Self {
        let path = crate::url::truncate_url_path(&url_path.into());
        Self {
            id: EventId(Uuid::now_v7()),
            website_id,
            session_id,
            visit_id,
            url_path: path,
            url_query: None,
            referrer_path: None,
            referrer_query: None,
            referrer_domain: None,
            page_title: None,
            event_type: 1,
            event_name: None,
            tag: None,
            hostname: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            gclid: None,
            fbclid: None,
            msclkid: None,
            ttclid: None,
            li_fat_id: None,
            twclid: None,
            cls: None,
            fcp: None,
            inp: None,
            lcp: None,
            ttfb: None,
            ttl: None,
            is_bot: false,
        }
    }
}

#[allow(clippy::fn_params_excessive_bools)]
pub fn determine_event_type(
    is_link: bool,
    is_pixel: bool,
    is_performance: bool,
    is_error: bool,
    has_name: bool,
) -> i32 {
    if is_link {
        crate::constants::EVENT_TYPE_LINK_EVENT
    } else if is_pixel {
        crate::constants::EVENT_TYPE_PIXEL_EVENT
    } else if is_performance {
        crate::constants::EVENT_TYPE_PERFORMANCE
    } else if is_error {
        crate::constants::EVENT_TYPE_ERROR
    } else if has_name {
        crate::constants::EVENT_TYPE_CUSTOM_EVENT
    } else {
        crate::constants::EVENT_TYPE_PAGE_VIEW
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDataValue(pub serde_json::Value);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicData {
    pub key: String,
    pub value: String,
    pub data_type: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageEngine {
    #[default]
    Postgres,
    Partitioned,
    Timescale,
    Clickhouse,
}

impl StorageEngine {
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "default" => Some(Self::Postgres),
            "partitioned" | "postgres-partitioned" | "partition" => Some(Self::Partitioned),
            "timescale" | "timescaledb" | "hypertable" => Some(Self::Timescale),
            "clickhouse" | "ch" => Some(Self::Clickhouse),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Partitioned => "partitioned",
            Self::Timescale => "timescale",
            Self::Clickhouse => "clickhouse",
        }
    }

    #[must_use]
    pub fn detect_from_lookup(get_var: &dyn Fn(&str) -> Option<String>) -> Self {
        if let Some(v) = get_var("STORAGE_ENGINE").or_else(|| get_var("ANALYTICS_STORAGE_ENGINE")) {
            if let Some(engine) = Self::parse_str(&v) {
                return engine;
            }
        }
        if let Some(ch_url) = get_var("CLICKHOUSE_URL") {
            if !ch_url.trim().is_empty() {
                return Self::Clickhouse;
            }
        }
        if get_var("TIMESCALE_ENABLED")
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        {
            return Self::Timescale;
        }
        Self::Postgres
    }

    #[must_use]
    pub fn detect_from_env() -> Self {
        Self::detect_from_lookup(&|k| std::env::var(k).ok())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn lookup<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            vars.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn test_types_creation_and_ser() {
        let u = Uuid::now_v7();
        let wid = WebsiteId(u);
        let sid = SessionId(u);
        let vid = VisitId(u);
        let eid = EventId(u);

        assert_eq!(wid.0, u);
        assert_eq!(sid.0, u);
        assert_eq!(vid.0, u);
        assert_eq!(eid.0, u);

        let json_wid = serde_json::to_string(&wid).unwrap();
        assert_eq!(json_wid, format!("\"{u}\""));

        let event = WebsiteEvent::new(wid, sid, vid, "/test/path");
        assert_eq!(event.url_path, "/test/path");
        assert_eq!(event.event_type, 1);
        let long = "p".repeat(600);
        let event2 = WebsiteEvent::new(wid, sid, vid, long);
        assert_eq!(event2.url_path.len(), 500);

        let dd = DynamicData {
            key: "test_key".into(),
            value: "test_val".into(),
            data_type: 1,
        };
        let edv = EventDataValue(serde_json::json!({"foo": "bar"}));
        assert_eq!(dd.key, "test_key");
        assert_eq!(edv.0["foo"], "bar");

        assert_eq!(determine_event_type(true, false, false, false, false), 3);
        assert_eq!(determine_event_type(false, true, false, false, false), 4);
        assert_eq!(determine_event_type(false, false, true, false, false), 5);
        assert_eq!(determine_event_type(false, false, false, true, false), 6);
        assert_eq!(determine_event_type(false, false, false, false, true), 2);
        assert_eq!(determine_event_type(false, false, false, false, false), 1);

        assert_eq!(StorageEngine::default(), StorageEngine::Postgres);
        assert_eq!(StorageEngine::parse_str("postgres"), Some(StorageEngine::Postgres));
        assert_eq!(StorageEngine::parse_str("postgresql"), Some(StorageEngine::Postgres));
        assert_eq!(StorageEngine::parse_str("default"), Some(StorageEngine::Postgres));
        assert_eq!(StorageEngine::parse_str("partitioned"), Some(StorageEngine::Partitioned));
        assert_eq!(StorageEngine::parse_str("postgres-partitioned"), Some(StorageEngine::Partitioned));
        assert_eq!(StorageEngine::parse_str("partition"), Some(StorageEngine::Partitioned));
        assert_eq!(StorageEngine::parse_str("timescale"), Some(StorageEngine::Timescale));
        assert_eq!(StorageEngine::parse_str("timescaledb"), Some(StorageEngine::Timescale));
        assert_eq!(StorageEngine::parse_str("hypertable"), Some(StorageEngine::Timescale));
        assert_eq!(StorageEngine::parse_str("clickhouse"), Some(StorageEngine::Clickhouse));
        assert_eq!(StorageEngine::parse_str("ch"), Some(StorageEngine::Clickhouse));
        assert_eq!(StorageEngine::parse_str("unknown"), None);

        assert_eq!(StorageEngine::Postgres.as_str(), "postgres");
        assert_eq!(StorageEngine::Partitioned.as_str(), "partitioned");
        assert_eq!(StorageEngine::Timescale.as_str(), "timescale");
        assert_eq!(StorageEngine::Clickhouse.as_str(), "clickhouse");

        let ser = serde_json::to_string(&StorageEngine::Clickhouse).unwrap();
        assert_eq!(ser, "\"clickhouse\"");
        let de: StorageEngine = serde_json::from_str("\"timescale\"").unwrap();
        assert_eq!(de, StorageEngine::Timescale);

        let detected = StorageEngine::detect_from_env();
        assert_eq!(
            StorageEngine::parse_str(detected.as_str()),
            Some(detected)
        );

        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[("STORAGE_ENGINE", "partitioned")])),
            StorageEngine::Partitioned
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[(
                "ANALYTICS_STORAGE_ENGINE",
                "timescale"
            )])),
            StorageEngine::Timescale
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[(
                "CLICKHOUSE_URL",
                "http://localhost:8123"
            )])),
            StorageEngine::Clickhouse
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[("CLICKHOUSE_URL", "   ")])),
            StorageEngine::Postgres
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[("TIMESCALE_ENABLED", "true")])),
            StorageEngine::Timescale
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[("TIMESCALE_ENABLED", "1")])),
            StorageEngine::Timescale
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[("TIMESCALE_ENABLED", "0")])),
            StorageEngine::Postgres
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[("STORAGE_ENGINE", "bogus-nope")])),
            StorageEngine::Postgres
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[
                ("STORAGE_ENGINE", "bogus-nope"),
                ("CLICKHOUSE_URL", "http://localhost:8123"),
            ])),
            StorageEngine::Clickhouse
        );
        assert_eq!(
            StorageEngine::detect_from_lookup(&lookup(&[])),
            StorageEngine::Postgres
        );
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_event_type_default() {
        let default_type = 1i32;
        kani::assert(default_type == 1, "pageview event type is 1");
    }

    #[kani::proof]
    fn harness_determine_event_type() {
        let is_link: bool = kani::any();
        let is_pixel: bool = kani::any();
        let is_performance: bool = kani::any();
        let is_error: bool = kani::any();
        let has_name: bool = kani::any();

        let t = super::determine_event_type(is_link, is_pixel, is_performance, is_error, has_name);
        kani::assert(t >= 1 && t <= 6, "event type in range 1..=6");
        if is_link {
            kani::assert(t == 3, "link event is 3");
        } else if is_pixel {
            kani::assert(t == 4, "pixel event is 4");
        } else if is_performance {
            kani::assert(t == 5, "performance event is 5");
        } else if is_error {
            kani::assert(t == 6, "error event is 6");
        } else if has_name {
            kani::assert(t == 2, "custom event is 2");
        } else {
            kani::assert(t == 1, "pageview event is 1");
        }
    }

    #[kani::proof]
    fn harness_storage_engine_default() {
        let engine = super::StorageEngine::default();
        kani::assert(engine == super::StorageEngine::Postgres, "default engine is postgres");
    }
}
