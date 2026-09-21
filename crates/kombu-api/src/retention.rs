#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

const ERR_FAILED_SAVE: &str = "Failed to save retention policy";
const DEFAULT_RETENTION_DAYS: i64 = 90;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicyInput {
    pub retention_days: i32,
    pub auto_purge_enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PurgeRequest {
    pub days: Option<i64>,
    pub before: Option<DateTime<Utc>>,
    pub url_path: Option<String>,
}

pub async fn get_policy(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let policy_row = sqlx::query_as::<_, (i32, bool, Option<DateTime<Utc>>)>(
        r#"
        SELECT retention_days, auto_purge_enabled, last_purged_at
        FROM "website_retention_policy"
        WHERE website_id = $1
        "#,
    )
    .bind(website_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (retention_days, auto_purge_enabled, last_purged_at) = match policy_row {
        Some((days, enabled, last)) => (days, enabled, last),
        None => (0, false, None),
    };

    let stats_row = sqlx::query_as::<_, (i64, Option<DateTime<Utc>>)>(
        r#"
        SELECT
            COUNT(*)::bigint,
            MIN(created_at)
        FROM "website_event"
        WHERE website_id = $1
        "#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, None));

    let total_events = stats_row.0;
    let oldest_event_at = stats_row.1;

    let purgeable_events = if retention_days > 0 {
        let cutoff = Utc::now() - Duration::days(i64::from(retention_days));
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::bigint
            FROM "website_event"
            WHERE website_id = $1 AND created_at < $2
            "#,
        )
        .bind(website_id)
        .bind(cutoff)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0)
    } else {
        0
    };

    Ok(Json(json!({
        "websiteId": website_id,
        "retentionDays": retention_days,
        "autoPurgeEnabled": auto_purge_enabled,
        "lastPurgedAt": last_purged_at,
        "stats": {
            "totalEvents": total_events,
            "oldestEventAt": oldest_event_at,
            "purgeableEvents": purgeable_events,
        }
    })))
}

pub async fn save_policy(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(input): Json<RetentionPolicyInput>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let now = Utc::now();
    let clamped_days = input.retention_days.max(0);

    sqlx::query(
        r#"
        INSERT INTO "website_retention_policy" (
            website_id, retention_days, auto_purge_enabled, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT (website_id) DO UPDATE
        SET retention_days = EXCLUDED.retention_days,
            auto_purge_enabled = EXCLUDED.auto_purge_enabled,
            updated_at = EXCLUDED.updated_at
        "#,
    )
    .bind(website_id)
    .bind(clamped_days)
    .bind(input.auto_purge_enabled)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": ERR_FAILED_SAVE, "details": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "websiteId": website_id,
        "retentionDays": clamped_days,
        "autoPurgeEnabled": input.auto_purge_enabled,
        "updatedAt": now
    })))
}

pub async fn purge(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<PurgeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let cutoff = if let Some(before) = body.before {
        before
    } else if let Some(days) = body.days {
        Utc::now() - Duration::days(days.max(1))
    } else {
        let policy_days = sqlx::query_scalar::<_, i32>(
            r#"SELECT retention_days FROM "website_retention_policy" WHERE website_id = $1"#,
        )
        .bind(website_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .unwrap_or(0);

        let days = if policy_days > 0 {
            i64::from(policy_days)
        } else {
            DEFAULT_RETENTION_DAYS
        };
        Utc::now() - Duration::days(days)
    };

    let purged_events = if let Some(ref path) = body.url_path {
        let pattern = if path.contains('%') {
            path.clone()
        } else {
            format!("{path}%")
        };
        sqlx::query(
            r#"
            DELETE FROM "website_event"
            WHERE website_id = $1
              AND url_path LIKE $2
              AND created_at < $3
            "#,
        )
        .bind(website_id)
        .bind(pattern)
        .bind(cutoff)
        .execute(&state.pool)
        .await
        .map_or(0, |res| res.rows_affected() as i64)
    } else {
        let _ = sqlx::query(
            r#"
            DELETE FROM "event_data"
            WHERE website_id = $1 AND created_at < $2
            "#,
        )
        .bind(website_id)
        .bind(cutoff)
        .execute(&state.pool)
        .await;

        sqlx::query(
            r#"
            DELETE FROM "website_event"
            WHERE website_id = $1 AND created_at < $2
            "#,
        )
        .bind(website_id)
        .bind(cutoff)
        .execute(&state.pool)
        .await
        .map_or(0, |res| res.rows_affected() as i64)
    };

    let purged_sessions = sqlx::query(
        r#"
        DELETE FROM "session"
        WHERE website_id = $1
          AND created_at < $2
          AND NOT EXISTS (
              SELECT 1 FROM "website_event" we WHERE we.session_id = "session".session_id
          )
        "#,
    )
    .bind(website_id)
    .bind(cutoff)
    .execute(&state.pool)
    .await
    .map_or(0, |res| res.rows_affected() as i64);

    let purge_id = Uuid::now_v7();
    let now = Utc::now();

    let _ = sqlx::query(
        r#"
        INSERT INTO "retention_purge_log" (
            purge_id, website_id, purged_events, purged_sessions, cutoff_date, filter_pattern, status, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6, 'completed', $7)
        "#,
    )
    .bind(purge_id)
    .bind(website_id)
    .bind(purged_events)
    .bind(purged_sessions)
    .bind(cutoff)
    .bind(&body.url_path)
    .bind(now)
    .execute(&state.pool)
    .await;

    let _ = sqlx::query(
        r#"
        UPDATE "website_retention_policy"
        SET last_purged_at = $2, updated_at = $2
        WHERE website_id = $1
        "#,
    )
    .bind(website_id)
    .bind(now)
    .execute(&state.pool)
    .await;

    Ok(Json(json!({
        "ok": true,
        "purgeId": purge_id,
        "websiteId": website_id,
        "purgedEvents": purged_events,
        "purgedSessions": purged_sessions,
        "cutoffDate": cutoff,
        "filterPattern": body.url_path
    })))
}

pub async fn history(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                purge_id as id,
                website_id as "websiteId",
                purged_events as "purgedEvents",
                purged_sessions as "purgedSessions",
                cutoff_date as "cutoffDate",
                filter_pattern as "filterPattern",
                status,
                created_at as "createdAt"
            FROM "retention_purge_log"
            WHERE website_id = $1
            ORDER BY created_at DESC
            LIMIT 50
        ) t
        "#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or_else(|_| json!([]));

    let count = rows.as_array().map_or(0, std::vec::Vec::len);

    Ok(Json(json!({
        "data": rows,
        "count": count
    })))
}

pub async fn admin_purge_all(
    _admin: crate::auth::AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let policies = sqlx::query_as::<_, (Uuid, i32)>(
        r#"
        SELECT website_id, retention_days
        FROM "website_retention_policy"
        WHERE auto_purge_enabled = true AND retention_days > 0
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut processed = 0;
    let mut total_purged_events = 0;
    let mut total_purged_sessions = 0;
    let now = Utc::now();

    for (site_id, days) in policies {
        let cutoff = now - Duration::days(i64::from(days));

        let purged_ev = sqlx::query(
            r#"
            DELETE FROM "website_event"
            WHERE website_id = $1 AND created_at < $2
            "#,
        )
        .bind(site_id)
        .bind(cutoff)
        .execute(&state.pool)
        .await
        .map_or(0, |res| res.rows_affected() as i64);

        let purged_sess = sqlx::query(
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
        .execute(&state.pool)
        .await
        .map_or(0, |res| res.rows_affected() as i64);

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
        .execute(&state.pool)
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
        .execute(&state.pool)
        .await;

        processed += 1;
        total_purged_events += purged_ev;
        total_purged_sessions += purged_sess;
    }

    Ok(Json(json!({
        "ok": true,
        "processedWebsites": processed,
        "totalPurgedEvents": total_purged_events,
        "totalPurgedSessions": total_purged_sessions,
        "executedAt": now
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retention_policy_and_purge_flow() {
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
            r#"INSERT INTO "website" (website_id, name, domain, created_at, updated_at) VALUES ($1, 'RetentionSite', 'retention.test', NOW(), NOW())"#,
        )
        .bind(website_id)
        .execute(&pool)
        .await
        .unwrap();

        let res_default = get_policy(Path(website_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_default.0["retentionDays"], 0);
        assert_eq!(res_default.0["autoPurgeEnabled"], false);

        let input = RetentionPolicyInput {
            retention_days: 90,
            auto_purge_enabled: true,
        };
        let res_save = save_policy(Path(website_id), State(state.clone()), Json(input))
            .await
            .unwrap();
        assert_eq!(res_save.0["retentionDays"], 90);
        assert_eq!(res_save.0["autoPurgeEnabled"], true);

        let res_get = get_policy(Path(website_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_get.0["retentionDays"], 90);
        assert_eq!(res_get.0["autoPurgeEnabled"], true);

        let session_id = Uuid::now_v7();
        let old_time = Utc::now() - Duration::days(100);
        sqlx::query(
            r#"
            INSERT INTO "session" (session_id, website_id, created_at)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(session_id)
        .bind(website_id)
        .bind(old_time)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, created_at)
            VALUES ($1, $2, $3, $4, '/old_page', $5)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(website_id)
        .bind(session_id)
        .bind(Uuid::now_v7())
        .bind(old_time)
        .execute(&pool)
        .await
        .unwrap();

        let purge_req_filtered = PurgeRequest {
            days: Some(90),
            before: None,
            url_path: Some("/old%".into()),
        };
        let res_filtered = purge(
            Path(website_id),
            State(state.clone()),
            Json(purge_req_filtered),
        )
        .await
        .unwrap();
        assert_eq!(res_filtered.0["ok"], true);
        assert_eq!(res_filtered.0["purgedEvents"], 1);

        sqlx::query(
            r#"
            INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, url_path, created_at)
            VALUES ($1, $2, $3, $4, '/old_page/sub', $5)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(website_id)
        .bind(session_id)
        .bind(Uuid::now_v7())
        .bind(old_time)
        .execute(&pool)
        .await
        .unwrap();
        let purge_req_plain = PurgeRequest {
            days: Some(90),
            before: None,
            url_path: Some("/old_page".into()),
        };
        let res_plain = purge(
            Path(website_id),
            State(state.clone()),
            Json(purge_req_plain),
        )
        .await
        .unwrap();
        assert_eq!(res_plain.0["ok"], true);
        assert_eq!(res_plain.0["purgedEvents"], 1);

        let purge_req_before = PurgeRequest {
            days: None,
            before: Some(Utc::now()),
            url_path: None,
        };
        let res_before = purge(
            Path(website_id),
            State(state.clone()),
            Json(purge_req_before),
        )
        .await
        .unwrap();
        assert_eq!(res_before.0["ok"], true);

        let purge_req_default = PurgeRequest {
            days: None,
            before: None,
            url_path: None,
        };
        let res_default_purge = purge(
            Path(website_id),
            State(state.clone()),
            Json(purge_req_default.clone()),
        )
        .await;
        assert!(res_default_purge.is_ok());

        let res_default_fallback = purge(
            Path(Uuid::now_v7()),
            State(state.clone()),
            Json(purge_req_default),
        )
        .await;
        assert!(res_default_fallback.is_ok());

        let res_hist = history(Path(website_id), State(state.clone()))
            .await
            .unwrap();
        assert!(res_hist.0["count"].as_i64().unwrap() >= 2);

        let admin_user = crate::auth::AdminUser(crate::auth::AuthUser {
            user_id: Uuid::now_v7(),
            username: "admin".into(),
            role: "admin".into(),
        });
        let res_admin = admin_purge_all(admin_user.clone(), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_admin.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "retention_purge_log" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "website_retention_policy" WHERE website_id = $1"#)
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

        assert!(save_policy(Path(website_id), State(err_state.clone()), Json(RetentionPolicyInput { retention_days: 30, auto_purge_enabled: false })).await.is_err());
        assert!(purge(Path(website_id), State(err_state.clone()), Json(PurgeRequest { days: Some(10), before: None, url_path: None })).await.is_ok());
        assert!(history(Path(website_id), State(err_state.clone())).await.is_ok());
        assert!(admin_purge_all(admin_user, State(err_state.clone())).await.is_ok());
    }
}
