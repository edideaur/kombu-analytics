#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::router::AppState;

pub fn build_realtime_payload(rows: &[Value]) -> Value {
    let mut countries: HashMap<String, i64> = HashMap::new();
    let mut urls: HashMap<String, i64> = HashMap::new();
    let mut referrers: HashMap<String, i64> = HashMap::new();
    let mut unique_sessions: HashSet<String> = HashSet::new();
    let mut processed_events = Vec::new();

    for item in rows {
        let session_id = item["sessionId"].as_str().unwrap_or("").to_string();
        let country = item["country"].as_str().unwrap_or("").to_string();
        let url_path = item["urlPath"].as_str().unwrap_or("").to_string();
        let referrer_domain = item["referrerDomain"].as_str().unwrap_or("").to_string();
        let event_name = item["eventName"].as_str();

        if !session_id.is_empty() && !unique_sessions.contains(&session_id) {
            unique_sessions.insert(session_id.clone());
            if !country.is_empty() {
                *countries.entry(country.clone()).or_insert(0) += 1;
            }
            let mut session_event = item.clone();
            let mut fallback = Map::new();
            let session_obj = session_event.as_object_mut().unwrap_or(&mut fallback);
            session_obj.insert("__type".into(), json!("session"));
            processed_events.push(session_event);
        }

        if !url_path.is_empty() {
            *urls.entry(url_path.clone()).or_insert(0) += 1;
        }
        if !referrer_domain.is_empty() {
            *referrers.entry(referrer_domain.clone()).or_insert(0) += 1;
        }

        let mut regular_event = item.clone();
        if let Some(o) = regular_event.as_object_mut() {
            let ev_type = if event_name.is_some() {
                "event"
            } else {
                "pageview"
            };
            o.insert("__type".into(), json!(ev_type));
        }
        processed_events.push(regular_event);
    }

    let views_count = rows.len() as i64;
    let visitors_count = unique_sessions.len() as i64;
    let countries_count = countries.len() as i64;
    let events_count = rows.iter().filter(|i| i["eventName"].is_string()).count() as i64;

    let mut map = Map::new();
    map.insert(
        "countries".into(),
        serde_json::to_value(&countries).unwrap_or_default(),
    );
    map.insert(
        "urls".into(),
        serde_json::to_value(&urls).unwrap_or_default(),
    );
    map.insert(
        "referrers".into(),
        serde_json::to_value(&referrers).unwrap_or_default(),
    );
    map.insert("events".into(), Value::Array(processed_events));

    let mut series = Map::new();
    series.insert("views".into(), Value::Array(Vec::new()));
    series.insert("visitors".into(), Value::Array(Vec::new()));
    map.insert("series".into(), Value::Object(series));

    let mut totals = Map::new();
    totals.insert("views".into(), Value::Number(views_count.into()));
    totals.insert("visitors".into(), Value::Number(visitors_count.into()));
    totals.insert("events".into(), Value::Number(events_count.into()));
    totals.insert("countries".into(), Value::Number(countries_count.into()));
    map.insert("totals".into(), Value::Object(totals));

    map.insert(
        "timestamp".into(),
        Value::Number(chrono::Utc::now().timestamp_millis().into()),
    );

    Value::Object(map)
}

pub async fn data(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = kombu_query::get_realtime_data(&state.pool, id, 30)
        .await
        .unwrap_or_default();

    let val = build_realtime_payload(&rows);
    Ok(Json(val))
}

pub fn create_realtime_stream(
    state: AppState,
    id: Uuid,
    interval_ms: u64,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    futures_util::stream::unfold((state, id, true), move |(state, id, is_first)| async move {
        if !is_first {
            tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;
        }
        let rows = kombu_query::get_realtime_data(&state.pool, id, 30)
            .await
            .unwrap_or_default();
        let payload = build_realtime_payload(&rows);
        let json_str = serde_json::to_string(&payload).unwrap_or_default();
        let event = Event::default().event("realtime").data(json_str);
        Some((Ok(event), (state, id, false)))
    })
}

pub async fn stream(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let s = create_realtime_stream(state, id, 2000);
    Sse::new(s).keep_alive(KeepAlive::default())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use futures_util::StreamExt;

    #[test]
    fn test_build_realtime_payload_branches() {
        let rows = vec![
            json!({
                "sessionId": "s1",
                "country": "US",
                "urlPath": "/home",
                "referrerDomain": "google.com",
                "eventName": "click"
            }),
            json!({
                "sessionId": "s1",
                "country": "US",
                "urlPath": "/pricing",
                "referrerDomain": "google.com"
            }),
            json!({
                "sessionId": "",
                "country": "",
                "urlPath": "",
                "referrerDomain": ""
            }),
            json!({
                "sessionId": "s2",
                "urlPath": "/about"
            }),
            json!("non-object-item"),
        ];

        let payload = build_realtime_payload(&rows);
        assert_eq!(payload["totals"]["views"], 5);
        assert_eq!(payload["totals"]["visitors"], 2);
        assert_eq!(payload["totals"]["events"], 1);
        assert_eq!(payload["totals"]["countries"], 1);
    }

    #[tokio::test]
    async fn test_realtime_endpoints_full() {
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

        let res_data = data(State(state.clone()), Path(website_id)).await;
        assert!(res_data.is_ok());

        let sse = stream(State(state.clone()), Path(website_id)).await;
        let resp = sse.into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        let stream = create_realtime_stream(state.clone(), website_id, 1);
        let mut pinned = std::pin::pin!(stream);

        let first = pinned.next().await;
        assert!(first.is_some());

        let second = pinned.next().await;
        assert!(second.is_some());
    }
}
