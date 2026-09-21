#![forbid(unsafe_code)]

use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use uuid::Uuid;

pub async fn evaluate_alerts_once(pool: &PgPool) {
    let rules = match sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            String,
            String,
            f64,
            String,
            Option<String>,
            Option<String>,
            i32,
            Option<chrono::DateTime<Utc>>,
        ),
    >(
        r#"
        SELECT
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, url_path, window_minutes, last_triggered_at
        FROM "alert_rule"
        WHERE enabled = true
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to fetch alert rules for evaluation: {e}");
            return;
        }
    };

    let now = Utc::now();

    for (
        alert_id,
        website_id,
        name,
        alert_type,
        webhook_url,
        channel_type,
        threshold,
        comparison,
        metric,
        url_path,
        window_minutes,
        last_triggered_at,
    ) in rules
    {
        if let Some(last) = last_triggered_at {
            let cooldown = Duration::minutes(i64::from(window_minutes.max(5)));
            if now - last < cooldown {
                continue;
            }
        }

        let window_start = now - Duration::minutes(i64::from(window_minutes.max(1)));
        let metric_str = metric.as_deref().unwrap_or("events");

        let current_value: f64 = if alert_type == "error_rate" || metric_str == "errors" {
            let count = if let Some(ref path) = url_path {
                sqlx::query_scalar::<_, i64>(
                    r#"
                    SELECT COUNT(*)::bigint
                    FROM "website_event"
                    WHERE website_id = $1 AND event_type = 2 AND url_path = $2 AND created_at >= $3
                    "#,
                )
                .bind(website_id)
                .bind(path)
                .bind(window_start)
                .fetch_one(pool)
                .await
                .unwrap_or(0)
            } else {
                sqlx::query_scalar::<_, i64>(
                    r#"
                    SELECT COUNT(*)::bigint
                    FROM "website_event"
                    WHERE website_id = $1 AND event_type = 2 AND created_at >= $2
                    "#,
                )
                .bind(website_id)
                .bind(window_start)
                .fetch_one(pool)
                .await
                .unwrap_or(0)
            };
            count as f64
        } else if alert_type == "performance_budget" {
            let avg = match metric_str {
                "cls" => sqlx::query_scalar::<_, Option<f64>>(
                    r#"
                        SELECT AVG(cls::float8)
                        FROM "website_event"
                        WHERE website_id = $1 AND cls IS NOT NULL AND created_at >= $2
                        "#,
                )
                .bind(website_id)
                .bind(window_start)
                .fetch_one(pool)
                .await
                .unwrap_or(None),
                "inp" => sqlx::query_scalar::<_, Option<f64>>(
                    r#"
                        SELECT AVG(inp::float8)
                        FROM "website_event"
                        WHERE website_id = $1 AND inp IS NOT NULL AND created_at >= $2
                        "#,
                )
                .bind(website_id)
                .bind(window_start)
                .fetch_one(pool)
                .await
                .unwrap_or(None),
                _ => sqlx::query_scalar::<_, Option<f64>>(
                    r#"
                        SELECT AVG(lcp::float8)
                        FROM "website_event"
                        WHERE website_id = $1 AND lcp IS NOT NULL AND created_at >= $2
                        "#,
                )
                .bind(website_id)
                .bind(window_start)
                .fetch_one(pool)
                .await
                .unwrap_or(None),
            };
            avg.unwrap_or(0.0)
        } else {
            let count = if let Some(ref path) = url_path {
                sqlx::query_scalar::<_, i64>(
                    r#"
                    SELECT COUNT(*)::bigint
                    FROM "website_event"
                    WHERE website_id = $1 AND url_path = $2 AND created_at >= $3
                    "#,
                )
                .bind(website_id)
                .bind(path)
                .bind(window_start)
                .fetch_one(pool)
                .await
                .unwrap_or(0)
            } else {
                sqlx::query_scalar::<_, i64>(
                    r#"
                    SELECT COUNT(*)::bigint
                    FROM "website_event"
                    WHERE website_id = $1 AND created_at >= $2
                    "#,
                )
                .bind(website_id)
                .bind(window_start)
                .fetch_one(pool)
                .await
                .unwrap_or(0)
            };
            count as f64
        };

        let is_triggered = if comparison == "lt" {
            current_value < threshold
        } else {
            current_value > threshold
        };

        if is_triggered {
            let msg = format!(
                "Alert rule '{name}' triggered for website {website_id}. Metric: {metric_str}, Value: {current_value:.2}, Threshold: {threshold:.2}"
            );
            tracing::info!(alert_id = %alert_id, name = %name, "Triggering alert webhook");

            let status = match crate::alerts::send_webhook_notification(
                &webhook_url,
                &channel_type,
                &name,
                &msg,
                current_value,
                threshold,
            )
            .await
            {
                Ok(()) => "sent",
                Err(err) => {
                    tracing::warn!("Webhook notification failed: {err}");
                    "failed"
                }
            };

            let history_id = Uuid::now_v7();
            let _ = sqlx::query(
                r#"
                INSERT INTO "alert_history" (
                    history_id, alert_id, website_id, alert_name, triggered_value, threshold, message, status, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
            )
            .bind(history_id)
            .bind(alert_id)
            .bind(website_id)
            .bind(&name)
            .bind(current_value)
            .bind(threshold)
            .bind(&msg)
            .bind(status)
            .bind(now)
            .execute(pool)
            .await;

            let _ = sqlx::query(
                r#"
                UPDATE "alert_rule"
                SET last_triggered_at = $2, updated_at = $2
                WHERE alert_id = $1
                "#,
            )
            .bind(alert_id)
            .bind(now)
            .execute(pool)
            .await;
        }
    }
}

pub async fn run_retention_purge_once(pool: &PgPool) {
    let policies = match sqlx::query_as::<_, (Uuid, i32)>(
        r#"
        SELECT website_id, retention_days
        FROM "website_retention_policy"
        WHERE auto_purge_enabled = true AND retention_days > 0
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to fetch retention policies for auto-purge: {e}");
            return;
        }
    };

    let now = Utc::now();

    for (site_id, days) in policies {
        let (purged_ev, purged_sess) = purge_site_retention(pool, site_id, days, now).await;
        let cutoff = now - Duration::days(i64::from(days));

        if purged_ev > 0 || purged_sess > 0 {
            tracing::info!(
                website_id = %site_id,
                purged_events = purged_ev,
                purged_sessions = purged_sess,
                "Retention auto-purge completed"
            );

            let purge_id = Uuid::now_v7();
            let _ = sqlx::query(
                r#"
                INSERT INTO "retention_purge_log" (
                    purge_id, website_id, purged_events, purged_sessions, cutoff_date, status, created_at
                ) VALUES ($1, $2, $3, $4, $5, 'completed', $6)
                "#,
            )
            .bind(purge_id)
            .bind(site_id)
            .bind(purged_ev)
            .bind(purged_sess)
            .bind(cutoff)
            .bind(now)
            .execute(pool)
            .await;

            let _ = sqlx::query(
                r#"
                UPDATE "website_retention_policy"
                SET last_purged_at = $2, updated_at = $2
                WHERE website_id = $1
                "#,
            )
            .bind(site_id)
            .bind(now)
            .execute(pool)
            .await;
        }
    }
}

pub async fn purge_site_retention(
    pool: &PgPool,
    site_id: Uuid,
    days: i32,
    now: chrono::DateTime<Utc>,
) -> (i64, i64) {
    let cutoff = now - Duration::days(i64::from(days));

    let purged_ev = match sqlx::query(
        r#"
        DELETE FROM "website_event"
        WHERE website_id = $1 AND created_at < $2
        "#,
    )
    .bind(site_id)
    .bind(cutoff)
    .execute(pool)
    .await
    {
        Ok(res) => res.rows_affected() as i64,
        Err(e) => {
            tracing::warn!("Failed to purge events for site {site_id}: {e}");
            0
        }
    };

    let purged_sess = match sqlx::query(
        r#"
        DELETE FROM "session"
        WHERE website_id = $1
          AND created_at < $2
          AND NOT EXISTS (
              SELECT 1 FROM "website_event" we WHERE we.session_id = "session".session_id
          )
        "#,
    )
    .bind(site_id)
    .bind(cutoff)
    .execute(pool)
    .await
    {
        Ok(res) => res.rows_affected() as i64,
        Err(e) => {
            tracing::warn!("Failed to purge sessions for site {site_id}: {e}");
            0
        }
    };

    (purged_ev, purged_sess)
}

pub fn start_background_tasks(pool: PgPool) -> tokio::task::JoinHandle<()> {
    start_background_tasks_with_durations(
        pool,
        StdDuration::from_secs(60),
        StdDuration::from_secs(3600),
    )
}

pub fn start_background_tasks_with_intervals(
    pool: PgPool,
    alert_secs: u64,
    retention_secs: u64,
) -> tokio::task::JoinHandle<()> {
    start_background_tasks_with_durations(
        pool,
        StdDuration::from_secs(alert_secs),
        StdDuration::from_secs(retention_secs),
    )
}
pub fn start_background_tasks_with_durations(
    pool: PgPool,
    alert_dur: StdDuration,
    retention_dur: StdDuration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut alert_interval = tokio::time::interval(alert_dur);
        let mut retention_interval = tokio::time::interval(retention_dur);
        while !pool.is_closed() {
            tokio::select! {
                biased;
                _ = retention_interval.tick() => {
                    run_retention_purge_once(&pool).await;
                }
                _ = alert_interval.tick() => {
                    evaluate_alerts_once(&pool).await;
                }
            }
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_background_task_starters() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

        let h1 = start_background_tasks(pool.clone());
        h1.abort();

        let h2 = start_background_tasks_with_intervals(pool.clone(), 1, 1);
        h2.abort();

        let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let h3 = start_background_tasks_with_durations(
            closed_pool.clone(),
            StdDuration::from_millis(10),
            StdDuration::from_millis(20),
        );
        tokio::time::sleep(StdDuration::from_millis(60)).await;
        closed_pool.close().await;
        let _ = h3.await;
    }
}
