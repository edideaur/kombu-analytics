#![allow(clippy::unwrap_used, clippy::expect_used)]

use uuid::Uuid;

#[tokio::test]
async fn test_evaluate_alerts_once_and_retention_purge() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

    let website_id = Uuid::now_v7();
    let site_name = format!("Site {}", website_id.simple());

    sqlx::query(
        r#"
        INSERT INTO "website" (website_id, name, domain, created_at)
        VALUES ($1, $2, 'bg-test.com', NOW())
        "#,
    )
    .bind(website_id)
    .bind(&site_name)
    .execute(&pool)
    .await
    .unwrap();

    let alert_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, window_minutes, enabled, created_at, updated_at
        ) VALUES ($1, $2, 'High Error Rate', 'error_rate', 'http://127.0.0.1:9/nowhere', 'generic', 0.0, 'gt', 'errors', 60, true, NOW(), NOW())
        "#,
    )
    .bind(alert_id)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let session_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, created_at)
        VALUES ($1, $2, 'Chrome', 'Linux', 'desktop', '1920x1080', 'en-US', 'US', NOW() - INTERVAL '10 minutes')
        "#,
    )
    .bind(session_id)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let visit_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, event_type, url_path, created_at)
        VALUES ($1, $2, $3, $4, 2, '/error-page', NOW() - INTERVAL '5 minutes')
        "#,
    )
    .bind(event_id)
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    kombu_api::background::evaluate_alerts_once(&pool).await;

    let history_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "alert_history" WHERE website_id = $1"#,
    )
    .bind(website_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(history_count >= 1);

    sqlx::query(
        r#"
        INSERT INTO "website_retention_policy" (
            website_id, retention_days, auto_purge_enabled, created_at, updated_at
        ) VALUES ($1, 1, true, NOW(), NOW())
        "#,
    )
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let old_session_id = Uuid::now_v7();
    let old_event_id = Uuid::now_v7();
    let old_date = chrono::Utc::now() - chrono::Duration::days(10);
    sqlx::query(
        r#"
        INSERT INTO "session" (session_id, website_id, browser, os, device, screen, language, country, created_at)
        VALUES ($1, $2, 'Chrome', 'Linux', 'desktop', '1920x1080', 'en-US', 'US', $3)
        "#,
    )
    .bind(old_session_id)
    .bind(website_id)
    .bind(old_date)
    .execute(&pool)
    .await
    .unwrap();

    let old_visit_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, event_type, url_path, created_at)
        VALUES ($1, $2, $3, $4, 1, '/old-page', $5)
        "#,
    )
    .bind(old_event_id)
    .bind(website_id)
    .bind(old_session_id)
    .bind(old_visit_id)
    .bind(old_date)
    .execute(&pool)
    .await
    .unwrap();

    kombu_api::background::run_retention_purge_once(&pool).await;

    let purge_log_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "retention_purge_log" WHERE website_id = $1"#,
    )
    .bind(website_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(purge_log_count >= 1);

    let alert_perf_cls = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, window_minutes, enabled, created_at, updated_at
        ) VALUES ($1, $2, 'Poor CLS', 'performance_budget', 'http://127.0.0.1:9/nowhere', 'generic', 0.05, 'gt', 'cls', 60, true, NOW(), NOW())
        "#,
    )
    .bind(alert_perf_cls)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let alert_perf_inp = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, window_minutes, enabled, created_at, updated_at
        ) VALUES ($1, $2, 'High INP', 'performance_budget', 'http://127.0.0.1:9/nowhere', 'generic', 100.0, 'gt', 'inp', 60, true, NOW(), NOW())
        "#,
    )
    .bind(alert_perf_inp)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let alert_perf_lcp = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, window_minutes, enabled, created_at, updated_at
        ) VALUES ($1, $2, 'Slow LCP', 'performance_budget', 'http://127.0.0.1:9/nowhere', 'generic', 2.0, 'gt', 'lcp', 60, true, NOW(), NOW())
        "#,
    )
    .bind(alert_perf_lcp)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let alert_traffic_drop = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, url_path, window_minutes, enabled, created_at, updated_at
        ) VALUES ($1, $2, 'Traffic Drop', 'traffic_spike', 'http://127.0.0.1:9/nowhere', 'generic', 1000.0, 'lt', 'events', '/popular', 60, true, NOW(), NOW())
        "#,
    )
    .bind(alert_traffic_drop)
    .bind(website_id)
    .execute(&pool)
    .await
    .unwrap();

    let perf_event_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "website_event" (event_id, website_id, session_id, visit_id, event_type, url_path, cls, inp, lcp, created_at)
        VALUES ($1, $2, $3, $4, 1, '/popular', 0.15, 250.0, 3.5, NOW() - INTERVAL '2 minutes')
        "#,
    )
    .bind(perf_event_id)
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .execute(&pool)
    .await
    .unwrap();

    kombu_api::background::evaluate_alerts_once(&pool).await;

    kombu_api::background::evaluate_alerts_once(&pool).await;

    let handle = kombu_api::background::start_background_tasks(pool.clone());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    handle.abort();

    let handle_int = kombu_api::background::start_background_tasks_with_durations(
        pool.clone(),
        std::time::Duration::from_millis(10),
        std::time::Duration::from_secs(3600),
    );
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    handle_int.abort();

    let handle_secs =
        kombu_api::background::start_background_tasks_with_intervals(pool.clone(), 1, 1);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    handle_secs.abort();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
            let _ = socket.write_all(resp.as_bytes()).await;
        }
    });

    let hook_url = format!("http://{local_addr}/hook");
    let alert_err_path = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, url_path, window_minutes, enabled, created_at, updated_at
        ) VALUES ($1, $2, 'Error Path', 'error_rate', $3, 'generic', 0.0, 'gt', 'errors', '/error-page', 60, true, NOW(), NOW())
        "#,
    )
    .bind(alert_err_path)
    .bind(website_id)
    .bind(&hook_url)
    .execute(&pool)
    .await
    .unwrap();

    let alert_events_nopath = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO "alert_rule" (
            alert_id, website_id, name, alert_type, webhook_url, channel_type,
            threshold, comparison, metric, window_minutes, enabled, created_at, updated_at
        ) VALUES ($1, $2, 'Events Total', 'traffic_spike', $3, 'generic', 0.0, 'gt', 'events', 60, true, NOW(), NOW())
        "#,
    )
    .bind(alert_events_nopath)
    .bind(website_id)
    .bind(&hook_url)
    .execute(&pool)
    .await
    .unwrap();

    kombu_api::background::evaluate_alerts_once(&pool).await;

    let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
    closed_pool.close().await;
    kombu_api::background::evaluate_alerts_once(&closed_pool).await;
    kombu_api::background::run_retention_purge_once(&closed_pool).await;
    let (ev, sess) = kombu_api::background::purge_site_retention(
        &closed_pool,
        website_id,
        1,
        chrono::Utc::now(),
    )
    .await;
    assert_eq!(ev, 0);
    assert_eq!(sess, 0);
}
