#![forbid(unsafe_code)]

use axum::Json;
use serde_json::{Map, Value};

pub fn get_with_lookup(get_var: &dyn Fn(&str) -> Option<String>) -> Json<Value> {
    let cloud_mode =
        get_var("CLOUD_MODE").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let private_mode =
        get_var("PRIVATE_MODE").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let telemetry_disabled =
        get_var("DISABLE_TELEMETRY").is_none_or(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let updates_disabled =
        get_var("DISABLE_UPDATES").is_none_or(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let tracker_script_name = get_var("TRACKER_SCRIPT_NAME")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "script.js".to_string());
    let favicon_url = get_var("FAVICON_URL");
    let links_url = get_var("LINKS_URL");
    let pixels_url = get_var("PIXELS_URL");
    let storage_engine = get_var("STORAGE_ENGINE")
        .or_else(|| get_var("ANALYTICS_STORAGE_ENGINE"))
        .unwrap_or_else(|| {
            if get_var("CLICKHOUSE_URL").is_some() {
                "clickhouse".to_string()
            } else if get_var("TIMESCALE_ENABLED")
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            {
                "timescale".to_string()
            } else {
                "postgres".to_string()
            }
        });
    let mut map = Map::new();
    map.insert("cloudMode".into(), Value::Bool(cloud_mode));
    map.insert(
        "faviconUrl".into(),
        favicon_url.map_or(Value::Null, Value::String),
    );
    map.insert(
        "linksUrl".into(),
        links_url.map_or(Value::Null, Value::String),
    );
    map.insert(
        "pixelsUrl".into(),
        pixels_url.map_or(Value::Null, Value::String),
    );
    map.insert("privateMode".into(), Value::Bool(private_mode));
    map.insert("sessionDeletionEnabled".into(), Value::Bool(true));
    map.insert("storageEngine".into(), Value::String(storage_engine));
    map.insert("telemetryDisabled".into(), Value::Bool(telemetry_disabled));
    map.insert(
        "trackerScriptName".into(),
        Value::String(tracker_script_name),
    );
    map.insert("updatesDisabled".into(), Value::Bool(updates_disabled));
    Json(Value::Object(map))
}

pub async fn get() -> Json<Value> {
    get_with_lookup(&|k| std::env::var(k).ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_get() {
        let res = get().await;
        let v = res.0;
        assert_eq!(v["sessionDeletionEnabled"], true);
        assert!(v.get("cloudMode").is_some());
        assert!(v.get("telemetryDisabled").is_some());

        let v2 = get_with_lookup(&|k| match k {
            "CLOUD_MODE" => Some("true".into()),
            "PRIVATE_MODE" => Some("1".into()),
            "DISABLE_TELEMETRY" => Some("false".into()),
            "DISABLE_UPDATES" => Some("0".into()),
            "TRACKER_SCRIPT_NAME" => Some("custom.js".into()),
            "FAVICON_URL" => Some("https://example.com/fav.ico".into()),
            "LINKS_URL" => Some("https://example.com/links".into()),
            _ => None,
        })
        .0;
        assert_eq!(v2["cloudMode"], true);
        assert_eq!(v2["privateMode"], true);
        assert_eq!(v2["telemetryDisabled"], false);
        assert_eq!(v2["updatesDisabled"], false);
        assert_eq!(v2["trackerScriptName"], "custom.js");
        assert_eq!(v2["pixelsUrl"], Value::Null);

        let v3 = get_with_lookup(&|k| match k {
            "CLOUD_MODE" => Some("0".into()),
            "PRIVATE_MODE" => Some("false".into()),
            "DISABLE_TELEMETRY" => Some("1".into()),
            "DISABLE_UPDATES" => Some("true".into()),
            "TRACKER_SCRIPT_NAME" => Some(String::new()),
            "PIXELS_URL" => Some("https://example.com/pixels".into()),
            _ => None,
        })
        .0;
        assert_eq!(v3["cloudMode"], false);
        assert_eq!(v3["privateMode"], false);
        assert_eq!(v3["telemetryDisabled"], true);
        assert_eq!(v3["updatesDisabled"], true);
        assert_eq!(v3["trackerScriptName"], "script.js");
        assert_eq!(v3["faviconUrl"], Value::Null);
        assert_eq!(v3["linksUrl"], Value::Null);

        let v4 = get_with_lookup(&|_| None).0;
        assert_eq!(v4["cloudMode"], false);
        assert_eq!(v4["telemetryDisabled"], true);
        assert_eq!(v4["storageEngine"], "postgres");

        let v5 = get_with_lookup(&|k| match k {
            "STORAGE_ENGINE" => Some("partitioned".into()),
            _ => None,
        })
        .0;
        assert_eq!(v5["storageEngine"], "partitioned");

        let v6 = get_with_lookup(&|k| match k {
            "CLICKHOUSE_URL" => Some("http://localhost:8123".into()),
            _ => None,
        })
        .0;
        assert_eq!(v6["storageEngine"], "clickhouse");

        let v7 = get_with_lookup(&|k| match k {
            "TIMESCALE_ENABLED" => Some("1".into()),
            _ => None,
        })
        .0;
        assert_eq!(v7["storageEngine"], "timescale");

        let v8 = get_with_lookup(&|k| match k {
            "TIMESCALE_ENABLED" => Some("true".into()),
            _ => None,
        })
        .0;
        assert_eq!(v8["storageEngine"], "timescale");

        let v9 = get_with_lookup(&|k| match k {
            "TIMESCALE_ENABLED" => Some("0".into()),
            _ => None,
        })
        .0;
        assert_eq!(v9["storageEngine"], "postgres");
    }
}
