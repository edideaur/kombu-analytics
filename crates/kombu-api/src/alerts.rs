#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertRuleInput {
    pub website_id: Uuid,
    pub name: String,
    pub alert_type: String,
    pub webhook_url: String,
    pub channel_type: Option<String>,
    pub threshold: f64,
    pub comparison: Option<String>,
    pub metric: Option<String>,
    pub url_path: Option<String>,
    pub window_minutes: Option<i32>,
    pub enabled: Option<bool>,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                alert_id as id,
                website_id as "websiteId",
                name,
                alert_type as "alertType",
                webhook_url as "webhookUrl",
                channel_type as "channelType",
                threshold,
                comparison,
                metric,
                url_path as "urlPath",
                window_minutes as "windowMinutes",
                enabled,
                last_triggered_at as "lastTriggeredAt",
                created_at as "createdAt"
            FROM "alert_rule"
            ORDER BY created_at DESC
        ) t
        "#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or_else(|_| json!([]));

    let count = rows.as_array().map_or(0, |a| a.len());

    Ok(Json(json!({
        "data": rows,
        "count": count,
        "page": 1,
        "pageSize": 100
    })))
}

pub async fn list_for_website(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                alert_id as id,
                website_id as "websiteId",
                name,
                alert_type as "alertType",
                webhook_url as "webhookUrl",
                channel_type as "channelType",
                threshold,
                comparison,
                metric,
                url_path as "urlPath",
                window_minutes as "windowMinutes",
                enabled,
                last_triggered_at as "lastTriggeredAt",
                created_at as "createdAt"
            FROM "alert_rule"
            WHERE website_id = $1
            ORDER BY created_at DESC
        ) t
        "#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or_else(|_| json!([]));

    let count = rows.as_array().map_or(0, |a| a.len());

    Ok(Json(json!({
        "data": rows,
        "count": count,
        "page": 1,
        "pageSize": 100
    })))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<AlertRuleInput>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let alert_id = Uuid::now_v7();
    let channel_type = body.channel_type.as_deref().unwrap_or("generic");
    let comparison = body.comparison.as_deref().unwrap_or("gt");
    let window_minutes = body.window_minutes.unwrap_or(60);
    let enabled = body.enabled.unwrap_or(true);

    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url,
            channel_type, threshold, comparison, metric, url_path,
            window_minutes, enabled, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW())
        "#,
    )
    .bind(alert_id)
    .bind(body.website_id)
    .bind(&body.name)
    .bind(&body.alert_type)
    .bind(&body.webhook_url)
    .bind(channel_type)
    .bind(body.threshold)
    .bind(comparison)
    .bind(&body.metric)
    .bind(&body.url_path)
    .bind(window_minutes)
    .bind(enabled)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(alert_id), State(state)).await
}

pub async fn get(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                alert_id as id,
                website_id as "websiteId",
                name,
                alert_type as "alertType",
                webhook_url as "webhookUrl",
                channel_type as "channelType",
                threshold,
                comparison,
                metric,
                url_path as "urlPath",
                window_minutes as "windowMinutes",
                enabled,
                last_triggered_at as "lastTriggeredAt",
                created_at as "createdAt"
            FROM "alert_rule"
            WHERE alert_id = $1
        ) t
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some(a) => Ok(Json(a)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Alert rule not found" })),
        )),
    }
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str();
    let webhook_url = body["webhookUrl"].as_str();
    let threshold = body["threshold"].as_f64();
    let enabled = body["enabled"].as_bool();

    sqlx::query(
        r#"
        UPDATE "alert_rule"
        SET
            name = COALESCE($2, name),
            webhook_url = COALESCE($3, webhook_url),
            threshold = COALESCE($4, threshold),
            enabled = COALESCE($5, enabled),
            updated_at = NOW()
        WHERE alert_id = $1
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(webhook_url)
    .bind(threshold)
    .bind(enabled)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(id), State(state)).await
}

pub async fn delete(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(r#"DELETE FROM "alert_rule" WHERE alert_id = $1"#)
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn send_webhook_notification(
    webhook_url: &str,
    channel_type: &str,
    alert_name: &str,
    message: &str,
    current_value: f64,
    threshold: f64,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    let payload = match channel_type {
        "discord" => json!({
            "embeds": [{
                "title": format!("🚨 Kombu Alert: {alert_name}"),
                "description": message,
                "color": 15_158_332,
                "fields": [
                    { "name": "Current Value", "value": format!("{current_value:.2}"), "inline": true },
                    { "name": "Threshold", "value": format!("{threshold:.2}"), "inline": true }
                ],
                "timestamp": Utc::now().to_rfc3339()
            }]
        }),
        "slack" => json!({
            "text": format!("🚨 *Kombu Alert: {alert_name}*\n{message}\n*Value:* {current_value:.2} | *Threshold:* {threshold:.2}")
        }),
        _ => json!({
            "event": "alert_triggered",
            "alert": alert_name,
            "message": message,
            "currentValue": current_value,
            "threshold": threshold,
            "timestamp": Utc::now().to_rfc3339()
        }),
    };

    let res = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(format!("Webhook responded with status {}", res.status()))
    }
}

pub async fn test_alert(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as::<_, (String, String, String, f64)>(
        r#"SELECT name, webhook_url, channel_type, threshold FROM "alert_rule" WHERE alert_id = $1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((name, webhook_url, channel_type, threshold)) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Alert rule not found" })),
        ));
    };

    let msg = format!("Test notification from Kombu Analytics for rule: {name}");
    let trigger_result = send_webhook_notification(
        &webhook_url,
        &channel_type,
        &name,
        &msg,
        threshold + 1.0,
        threshold,
    )
    .await;

    match trigger_result {
        Ok(()) => Ok(Json(
            json!({ "ok": true, "message": "Webhook sent successfully" }),
        )),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("Webhook dispatch failed: {e}") })),
        )),
    }
}

pub async fn list_history(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                history_id as id,
                alert_id as "alertId",
                website_id as "websiteId",
                alert_name as "alertName",
                triggered_value as "triggeredValue",
                threshold,
                message,
                status,
                created_at as "createdAt"
            FROM "alert_history"
            WHERE website_id = $1
            ORDER BY created_at DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or_else(|_| json!([]));

    Ok(Json(json!({ "data": rows })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alerts_full_crud_and_notifications() {
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
            r#"INSERT INTO "website" (website_id, name, domain, created_at) VALUES ($1, 'AlertsTestSite', 'alerts.test', NOW())"#,
        )
        .bind(website_id)
        .execute(&pool)
        .await
        .unwrap();

        let input = AlertRuleInput {
            website_id,
            name: "Spike Alert".into(),
            alert_type: "traffic_spike".into(),
            webhook_url: "http://127.0.0.1:9/webhook".into(),
            channel_type: Some("generic".into()),
            threshold: 100.0,
            comparison: Some("gt".into()),
            metric: Some("events".into()),
            url_path: Some("/home".into()),
            window_minutes: Some(15),
            enabled: Some(true),
        };
        let res_create = create(State(state.clone()), Json(input.clone()))
            .await
            .unwrap();
        let alert_id_str = res_create.0["id"].as_str().unwrap();
        let alert_id = Uuid::parse_str(alert_id_str).unwrap();
        assert_eq!(res_create.0["name"], "Spike Alert");

        let res_list = list(State(state.clone())).await.unwrap();
        assert!(res_list.0["count"].as_i64().unwrap() >= 1);

        let res_site_list = list_for_website(Path(website_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_site_list.0["count"], 1);

        let res_get = get(Path(alert_id), State(state.clone())).await.unwrap();
        assert_eq!(res_get.0["name"], "Spike Alert");

        let res_get_404 = get(Path(Uuid::now_v7()), State(state.clone())).await;
        assert!(res_get_404.is_err());
        assert_eq!(res_get_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_up = update(
            Path(alert_id),
            State(state.clone()),
            Json(json!({
                "name": "Updated Spike Alert",
                "threshold": 150.0,
                "enabled": false
            })),
        )
        .await
        .unwrap();
        assert_eq!(res_up.0["name"], "Updated Spike Alert");
        assert_eq!(res_up.0["threshold"], 150.0);
        assert_eq!(res_up.0["enabled"], false);

        let res_test = test_alert(Path(alert_id), State(state.clone())).await;
        assert!(res_test.is_err());
        assert_eq!(res_test.unwrap_err().0, StatusCode::BAD_GATEWAY);

        let res_test_404 = test_alert(Path(Uuid::now_v7()), State(state.clone())).await;
        assert!(res_test_404.is_err());
        assert_eq!(res_test_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let _ = send_webhook_notification(
            "http://127.0.0.1:9/discord",
            "discord",
            "Test",
            "Msg",
            10.0,
            5.0,
        )
        .await;
        let _ = send_webhook_notification(
            "http://127.0.0.1:9/slack",
            "slack",
            "Test",
            "Msg",
            10.0,
            5.0,
        )
        .await;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let mock_app = axum::Router::new()
            .route(
                "/ok",
                axum::routing::post(|| async { (StatusCode::OK, "ok") }),
            )
            .route(
                "/err",
                axum::routing::post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "err") }),
            );
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let mock_srv = tokio::spawn(async move {
            let _ = axum::serve(listener, mock_app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        let ok_url = format!("http://{local_addr}/ok");
        let err_url = format!("http://{local_addr}/err");

        assert!(
            send_webhook_notification(&ok_url, "discord", "Test", "Msg", 10.0, 5.0)
                .await
                .is_ok()
        );
        assert!(
            send_webhook_notification(&err_url, "slack", "Test", "Msg", 10.0, 5.0)
                .await
                .is_err()
        );

        let _ = update(
            Path(alert_id),
            State(state.clone()),
            Json(json!({ "webhookUrl": ok_url })),
        )
        .await;
        let res_test_ok = test_alert(Path(alert_id), State(state.clone())).await;
        assert!(res_test_ok.is_ok());
        let _ = shutdown_tx.send(());
        let _ = mock_srv.await;

        let res_history = list_history(Path(website_id), State(state.clone()))
            .await
            .unwrap();
        assert!(res_history.0["data"].is_array());

        let res_del = delete(Path(alert_id), State(state.clone())).await.unwrap();
        assert_eq!(res_del.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;

        let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        closed_pool.close().await;
        let err_state = AppState {
            pool: closed_pool,
            session_cache: state.session_cache.clone(),
            website_cache: state.website_cache.clone(),
            rate_limiter: state.rate_limiter.clone(),
            ingest_queue: state.ingest_queue.clone(),
            app_secret: state.app_secret.clone(),
        };

        assert!(list(State(err_state.clone())).await.is_ok());
        assert!(
            list_for_website(Path(website_id), State(err_state.clone()))
                .await
                .is_ok()
        );
        assert!(
            list_history(Path(website_id), State(err_state.clone()))
                .await
                .is_ok()
        );
        assert!(create(State(err_state.clone()), Json(input)).await.is_err());
        assert!(get(Path(alert_id), State(err_state.clone())).await.is_err());
        assert!(
            update(Path(alert_id), State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
        assert!(
            delete(Path(alert_id), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
