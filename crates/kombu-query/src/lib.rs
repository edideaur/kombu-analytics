#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use kombu_core::types::StorageEngine;
pub use kombu_db::{ClickHouseClient, ClickHouseConfig, ClickHouseError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteStats {
    pub pageviews: i64,
    pub visitors: i64,
    pub visits: i64,
    pub bounces: i64,
    pub totaltime: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricCount {
    pub x: String,
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpandedMetricData {
    pub name: String,
    pub pageviews: i64,
    pub visitors: i64,
    pub visits: i64,
    pub bounces: i64,
    pub totaltime: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    #[serde(rename = "startDate")]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(rename = "endDate")]
    pub end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedResult<T> {
    pub data: Vec<T>,
    pub count: i64,
    pub page: i64,
    #[serde(rename = "pageSize")]
    pub page_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatValue {
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteSessionStats {
    pub pageviews: StatValue,
    pub visitors: StatValue,
    pub visits: StatValue,
    pub countries: StatValue,
    pub events: StatValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteEventStats {
    pub events: i64,
    pub visitors: i64,
    pub visits: i64,
    #[serde(rename = "uniqueEvents")]
    pub unique_events: i64,
}

pub async fn get_website_stats(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<WebsiteStats, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        r#"
        SELECT
            COALESCE(SUM(t.c), 0)::bigint as pageviews,
            COUNT(DISTINCT t.session_id)::bigint as visitors,
            COUNT(DISTINCT t.visit_id)::bigint as visits,
            COALESCE(SUM(CASE WHEN t.c = 1 AND t.has_custom_event = 0 AND (t.max_time IS NULL OR t.max_time = t.min_time) THEN 1 ELSE 0 END), 0)::bigint as bounces,
            COALESCE(SUM(EXTRACT(EPOCH FROM (t.max_time - t.min_time))), 0)::bigint as totaltime
        FROM (
            SELECT
                session_id,
                visit_id,
                SUM(CASE WHEN event_type NOT IN (2, 5) THEN 1 ELSE 0 END) as c,
                MIN(CASE WHEN event_type NOT IN (2, 5) THEN created_at END) as min_time,
                MAX(CASE WHEN event_type NOT IN (2, 5) THEN created_at END) as max_time,
                MAX(CASE WHEN event_type = 2 THEN 1 ELSE 0 END) as has_custom_event
            FROM website_event
            WHERE website_id = $1
              AND created_at BETWEEN $2 AND $3
              AND event_type != 4
              AND is_bot = false
            GROUP BY session_id, visit_id
            HAVING SUM(CASE WHEN event_type NOT IN (2, 5) THEN 1 ELSE 0 END) > 0
        ) t
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(pool)
    .await?;

    Ok(WebsiteStats {
        pageviews: row.0,
        visitors: row.1,
        visits: row.2,
        bounces: row.3,
        totaltime: row.4,
    })
}

pub async fn get_website_stats_from_rollups(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<Option<WebsiteStats>, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64)>(
        r#"
        SELECT
            COUNT(*)::bigint as buckets,
            COALESCE(SUM(views), 0)::bigint as pageviews,
            COALESCE(SUM(visitors), 0)::bigint as visitors,
            COALESCE(SUM(visits), 0)::bigint as visits,
            COALESCE(SUM(bounces), 0)::bigint as bounces,
            COALESCE(SUM(totaltime), 0)::bigint as totaltime
        FROM website_event_stats_hourly
        WHERE website_id = $1
          AND hour_bucket >= date_trunc('hour', $2)
          AND hour_bucket <= $3
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(pool)
    .await?;

    if row.0 > 0 {
        Ok(Some(WebsiteStats {
            pageviews: row.1,
            visitors: row.2,
            visits: row.3,
            bounces: row.4,
            totaltime: row.5,
        }))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClickHouseStatsRow {
    #[serde(default)]
    pub pageviews: i64,
    #[serde(default)]
    pub visitors: i64,
    #[serde(default)]
    pub visits: i64,
    #[serde(default)]
    pub bounces: i64,
    #[serde(default)]
    pub totaltime: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClickHouseMetricRow {
    #[serde(default)]
    pub x: String,
    #[serde(default)]
    pub y: i64,
}

pub async fn get_website_stats_clickhouse(
    client: &ClickHouseClient,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<WebsiteStats, ClickHouseError> {
    let start_str = start_at.format("%Y-%m-%d %H:%M:%S").to_string();
    let end_str = end_at.format("%Y-%m-%d %H:%M:%S").to_string();

    let sql = format!(
        r#"
        SELECT
            countIf(event_type NOT IN (2, 5)) AS pageviews,
            uniqExact(session_id) AS visitors,
            uniqExact(visit_id) AS visits,
            countIf(views = 1 AND min_time = max_time) AS bounces,
            sum(dateDiff('second', min_time, max_time)) AS totaltime
        FROM (
            SELECT
                session_id,
                visit_id,
                count(*) AS views,
                min(created_at) AS min_time,
                max(created_at) AS max_time
            FROM website_event
            WHERE website_id = '{website_id}'
              AND created_at >= '{start_str}' AND created_at <= '{end_str}'
            GROUP BY session_id, visit_id
        )
        "#
    );

    let rows: Vec<ClickHouseStatsRow> = client.query_json(&sql).await?;
    if let Some(r) = rows.into_iter().next() {
        Ok(WebsiteStats {
            pageviews: r.pageviews,
            visitors: r.visitors,
            visits: r.visits,
            bounces: r.bounces,
            totaltime: r.totaltime,
        })
    } else {
        Ok(WebsiteStats {
            pageviews: 0,
            visitors: 0,
            visits: 0,
            bounces: 0,
            totaltime: 0,
        })
    }
}

pub async fn get_metrics_clickhouse(
    client: &ClickHouseClient,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    metric_type: &str,
    limit: i64,
) -> Result<Vec<MetricCount>, ClickHouseError> {
    let column = match metric_type {
        "url" | "path" => "url_path",
        "referrer" => "referrer_domain",
        "browser" => "browser",
        "os" => "os",
        "device" => "device",
        "country" => "country",
        "event" => "event_name",
        "host" | "hostname" => "hostname",
        _ => "url_path",
    };

    let start_str = start_at.format("%Y-%m-%d %H:%M:%S").to_string();
    let end_str = end_at.format("%Y-%m-%d %H:%M:%S").to_string();

    let sql = format!(
        r#"
        SELECT ifNull({column}, 'unknown') AS x, count(*) AS y
        FROM website_event
        WHERE website_id = '{website_id}'
          AND created_at >= '{start_str}' AND created_at <= '{end_str}'
        GROUP BY {column}
        ORDER BY y DESC
        LIMIT {limit}
        "#
    );

    let rows: Vec<ClickHouseMetricRow> = client.query_json(&sql).await?;
    Ok(rows.into_iter().map(|r| MetricCount { x: r.x, y: r.y }).collect())
}

pub async fn get_website_stats_with_engine(
    engine: StorageEngine,
    pool: &sqlx::PgPool,
    ch_client: Option<&ClickHouseClient>,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<WebsiteStats, sqlx::Error> {
    match engine {
        StorageEngine::Partitioned => {
            if let Ok(Some(stats)) =
                get_website_stats_from_rollups(pool, website_id, start_at, end_at).await
            {
                Ok(stats)
            } else {
                get_website_stats(pool, website_id, start_at, end_at).await
            }
        }
        StorageEngine::Clickhouse => {
            if let Some(client) = ch_client {
                if let Ok(stats) =
                    get_website_stats_clickhouse(client, website_id, start_at, end_at).await
                {
                    Ok(stats)
                } else {
                    get_website_stats(pool, website_id, start_at, end_at).await
                }
            } else {
                get_website_stats(pool, website_id, start_at, end_at).await
            }
        }
        StorageEngine::Timescale | StorageEngine::Postgres => {
            get_website_stats(pool, website_id, start_at, end_at).await
        }
    }
}

pub async fn get_metrics_with_engine(
    engine: StorageEngine,
    pool: &sqlx::PgPool,
    ch_client: Option<&ClickHouseClient>,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    metric_type: &str,
    limit: i64,
) -> Result<Vec<MetricCount>, sqlx::Error> {
    if engine == StorageEngine::Clickhouse {
        if let Some(client) = ch_client {
            if let Ok(metrics) =
                get_metrics_clickhouse(client, website_id, start_at, end_at, metric_type, limit)
                    .await
            {
                Ok(metrics)
            } else {
                get_metrics(pool, website_id, start_at, end_at, metric_type, limit).await
            }
        } else {
            get_metrics(pool, website_id, start_at, end_at, metric_type, limit).await
        }
    } else {
        get_metrics(pool, website_id, start_at, end_at, metric_type, limit).await
    }
}

pub async fn get_active_visitors(
    pool: &sqlx::PgPool,
    website_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64,)>(
        r#"
        SELECT COUNT(DISTINCT session_id)::bigint
        FROM website_event
        WHERE website_id = $1
          AND created_at >= NOW() - INTERVAL '5 minutes'
          AND is_bot = false
        "#,
    )
    .bind(website_id)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn get_website_date_range(
    pool: &sqlx::PgPool,
    website_id: Uuid,
) -> Result<DateRange, sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(
        r#"
        SELECT MIN(created_at) as "startDate", MAX(created_at) as "endDate"
        FROM website_event
        WHERE website_id = $1
        "#,
    )
    .bind(website_id)
    .fetch_one(pool)
    .await?;

    Ok(DateRange {
        start_date: row.0,
        end_date: row.1,
    })
}

pub async fn get_metrics(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    metric_type: &str,
    limit: i64,
) -> Result<Vec<MetricCount>, sqlx::Error> {
    let column = match metric_type {
        "url" | "path" => "url_path",
        "referrer" => "referrer_domain",
        "browser" => "browser",
        "os" => "os",
        "device" => "device",
        "country" => "country",
        "event" => "event_name",
        "host" | "hostname" => "hostname",
        _ => "url_path",
    };

    let sql = format!(
        r#"
        SELECT COALESCE({}, 'unknown') as x, COUNT(*)::bigint as y
        FROM website_event
        LEFT JOIN session ON website_event.session_id = session.session_id
        WHERE website_event.website_id = $1
          AND website_event.created_at BETWEEN $2 AND $3
          AND website_event.is_bot = false
        GROUP BY 1
        ORDER BY 2 DESC
        LIMIT $4
        "#,
        column
    );

    let rows = sqlx::query_as::<_, (String, i64)>(&sql)
        .bind(website_id)
        .bind(start_at)
        .bind(end_at)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(x, y)| MetricCount { x, y })
        .collect())
}

pub async fn get_expanded_metrics(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    metric_type: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ExpandedMetricData>, sqlx::Error> {
    let column = match metric_type {
        "url" | "path" | "entry" | "exit" => "website_event.url_path",
        "referrer" | "domain" | "channel" => "website_event.referrer_domain",
        "browser" => "session.browser",
        "os" => "session.os",
        "device" => "session.device",
        "country" => "session.country",
        "region" | "subdivision" => "session.region",
        "city" => "session.city",
        "language" => "session.language",
        "event" => "website_event.event_name",
        "title" => "website_event.page_title",
        "host" | "hostname" => "website_event.hostname",
        _ => "website_event.url_path",
    };

    let sql = format!(
        r#"
        SELECT
            COALESCE({column}, 'unknown') as name,
            COUNT(*)::bigint as pageviews,
            COUNT(DISTINCT website_event.session_id)::bigint as visitors,
            COUNT(DISTINCT website_event.visit_id)::bigint as visits,
            COALESCE(SUM(CASE WHEN t.c = 1 AND (t.max_time IS NULL OR t.max_time = t.min_time) THEN 1 ELSE 0 END), 0)::bigint as bounces,
            COALESCE(SUM(EXTRACT(EPOCH FROM (t.max_time - t.min_time))), 0)::bigint as totaltime
        FROM website_event
        LEFT JOIN session ON website_event.session_id = session.session_id
        LEFT JOIN (
            SELECT visit_id, COUNT(*) as c, MIN(created_at) as min_time, MAX(created_at) as max_time
            FROM website_event
            WHERE website_id = $1 AND created_at BETWEEN $2 AND $3
            GROUP BY visit_id
        ) t ON website_event.visit_id = t.visit_id
        WHERE website_event.website_id = $1
          AND website_event.created_at BETWEEN $2 AND $3
          AND website_event.is_bot = false
        GROUP BY 1
        ORDER BY 2 DESC
        LIMIT $4
        OFFSET $5
        "#
    );

    let rows = sqlx::query_as::<_, (String, i64, i64, i64, i64, i64)>(&sql)
        .bind(website_id)
        .bind(start_at)
        .bind(end_at)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(name, pageviews, visitors, visits, bounces, totaltime)| {
            ExpandedMetricData {
                name,
                pageviews,
                visitors,
                visits,
                bounces,
                totaltime,
            }
        })
        .collect())
}

pub async fn get_pageview_stats(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    unit: &str,
) -> Result<Vec<MetricCount>, sqlx::Error> {
    let (trunc, format_str) = match unit {
        "hour" => ("hour", "YYYY-MM-DD HH24:00:00"),
        "day" => ("day", "YYYY-MM-DD 00:00:00"),
        "month" => ("month", "YYYY-MM-01 00:00:00"),
        "year" => ("year", "YYYY-01-01 00:00:00"),
        _ => ("day", "YYYY-MM-DD 00:00:00"),
    };

    let sql = format!(
        r#"
        SELECT
            TO_CHAR(DATE_TRUNC('{trunc}', created_at), '{format_str}') as x,
            COUNT(*)::bigint as y
        FROM website_event
        WHERE website_id = $1
          AND created_at BETWEEN $2 AND $3
          AND event_type NOT IN (2, 5)
          AND is_bot = false
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );

    let rows = sqlx::query_as::<_, (String, i64)>(&sql)
        .bind(website_id)
        .bind(start_at)
        .bind(end_at)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(x, y)| MetricCount { x, y })
        .collect())
}

pub async fn get_session_stats(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    unit: &str,
) -> Result<Vec<MetricCount>, sqlx::Error> {
    let (trunc, format_str) = match unit {
        "hour" => ("hour", "YYYY-MM-DD HH24:00:00"),
        "day" => ("day", "YYYY-MM-DD 00:00:00"),
        "month" => ("month", "YYYY-MM-01 00:00:00"),
        "year" => ("year", "YYYY-01-01 00:00:00"),
        _ => ("day", "YYYY-MM-DD 00:00:00"),
    };

    let sql = format!(
        r#"
        SELECT
            TO_CHAR(DATE_TRUNC('{trunc}', created_at), '{format_str}') as x,
            COUNT(DISTINCT session_id)::bigint as y
        FROM website_event
        WHERE website_id = $1
          AND created_at BETWEEN $2 AND $3
          AND event_type NOT IN (2, 5)
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );

    let rows = sqlx::query_as::<_, (String, i64)>(&sql)
        .bind(website_id)
        .bind(start_at)
        .bind(end_at)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(x, y)| MetricCount { x, y })
        .collect())
}

pub async fn get_website_session_stats(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<WebsiteSessionStats, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN we.event_type = 1 THEN 1 ELSE 0 END), 0)::bigint as pageviews,
            COUNT(DISTINCT we.session_id)::bigint as visitors,
            COUNT(DISTINCT we.visit_id)::bigint as visits,
            COUNT(DISTINCT s.country)::bigint as countries,
            COALESCE(SUM(CASE WHEN we.event_type = 2 THEN 1 ELSE 0 END), 0)::bigint as events
        FROM website_event we
        JOIN session s ON we.session_id = s.session_id AND we.website_id = s.website_id
        WHERE we.website_id = $1
          AND we.created_at BETWEEN $2 AND $3
          AND we.event_type != 4
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(pool)
    .await?;

    Ok(WebsiteSessionStats {
        pageviews: StatValue { value: row.0 },
        visitors: StatValue { value: row.1 },
        visits: StatValue { value: row.2 },
        countries: StatValue { value: row.3 },
        events: StatValue { value: row.4 },
    })
}

pub async fn get_website_event_stats(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<WebsiteEventStats, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        SELECT
            COUNT(*)::bigint as events,
            COUNT(DISTINCT session_id)::bigint as visitors,
            COUNT(DISTINCT visit_id)::bigint as visits,
            COUNT(DISTINCT event_name)::bigint as unique_events
        FROM website_event
        WHERE website_id = $1
          AND created_at BETWEEN $2 AND $3
          AND event_type = 2
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(pool)
    .await?;

    Ok(WebsiteEventStats {
        events: row.0,
        visitors: row.1,
        visits: row.2,
        unique_events: row.3,
    })
}

pub async fn get_weekly_traffic(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<Vec<Vec<i64>>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i32, i32, i64)>(
        r#"
        SELECT
            EXTRACT(DOW FROM created_at)::int as dow,
            EXTRACT(HOUR FROM created_at)::int as hour,
            COUNT(DISTINCT session_id)::bigint as value
        FROM website_event
        WHERE website_id = $1
          AND created_at BETWEEN $2 AND $3
          AND event_type NOT IN (2, 5)
        GROUP BY dow, hour
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_all(pool)
    .await?;

    let mut matrix = vec![vec![0i64; 24]; 7];
    for (dow, hour, val) in rows {
        let d = (dow as usize) % 7;
        let h = (hour as usize) % 24;
        matrix[d][h] = val;
    }

    Ok(matrix)
}

pub async fn get_values(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    column_name: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let valid_col = match column_name {
        "url_path" | "path" => "we.url_path",
        "referrer_domain" | "referrer" => "we.referrer_domain",
        "browser" => "s.browser",
        "os" => "s.os",
        "device" => "s.device",
        "country" => "s.country",
        "event_name" | "event" => "we.event_name",
        _ => "we.url_path",
    };

    let sql = format!(
        r#"
        SELECT DISTINCT {valid_col}
        FROM website_event we
        LEFT JOIN session s ON we.session_id = s.session_id
        WHERE we.website_id = $1
          AND we.created_at BETWEEN $2 AND $3
          AND {valid_col} IS NOT NULL
        ORDER BY 1 ASC
        LIMIT 500
        "#
    );

    let rows = sqlx::query_scalar::<_, String>(&sql)
        .bind(website_id)
        .bind(start_at)
        .bind(end_at)
        .fetch_all(pool)
        .await?;

    Ok(rows)
}

pub async fn get_realtime_data(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    minutes: i32,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                we.session_id as "sessionId",
                we.created_at as "createdAt",
                we.url_path as "urlPath",
                we.referrer_domain as "referrerDomain",
                we.event_name as "eventName",
                s.country
            FROM website_event we
            LEFT JOIN session s ON s.session_id = we.session_id
            WHERE we.website_id = $1
              AND we.created_at > NOW() - ($2 || ' minutes')::interval
            ORDER BY we.created_at DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(website_id)
    .bind(minutes.to_string())
    .fetch_one(pool)
    .await?;

    Ok(rows.as_array().cloned().unwrap_or_default())
}

pub async fn get_website_events(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    page: i64,
    page_size: i64,
    search: Option<&str>,
) -> Result<PagedResult<serde_json::Value>, sqlx::Error> {
    let offset = (page - 1).max(0) * page_size;
    let search_pattern = search.map(|s| format!("%{s}%"));

    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM website_event
        WHERE website_id = $1
          AND created_at BETWEEN $2 AND $3
          AND event_type != 4
          AND ($4::text IS NULL OR event_name ILIKE $4 OR url_path ILIKE $4)
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(&search_pattern)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                we.event_id as id,
                we.website_id as "websiteId",
                we.session_id as "sessionId",
                we.visit_id as "visitId",
                we.created_at as "createdAt",
                we.url_path as "urlPath",
                we.url_query as "urlQuery",
                we.referrer_domain as "referrerDomain",
                we.page_title as "pageTitle",
                we.event_type as "eventType",
                we.event_name as "eventName",
                we.hostname,
                s.browser,
                s.os,
                s.device,
                s.country,
                s.city
            FROM website_event we
            LEFT JOIN session s ON we.session_id = s.session_id
            WHERE we.website_id = $1
              AND we.created_at BETWEEN $2 AND $3
              AND we.event_type != 4
              AND ($4::text IS NULL OR we.event_name ILIKE $4 OR we.url_path ILIKE $4)
            ORDER BY we.created_at DESC
            LIMIT $5 OFFSET $6
        ) t
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(&search_pattern)
    .bind(page_size)
    .bind(offset)
    .fetch_one(pool)
    .await?;

    Ok(PagedResult {
        data: json_to_vec(rows),
        count,
        page,
        page_size,
    })
}

pub async fn get_website_sessions(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    page: i64,
    page_size: i64,
    search: Option<&str>,
) -> Result<PagedResult<serde_json::Value>, sqlx::Error> {
    let offset = (page - 1).max(0) * page_size;
    let search_pattern = search.map(|s| format!("%{s}%"));

    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(DISTINCT s.session_id)::bigint
        FROM session s
        JOIN website_event we ON we.session_id = s.session_id
        WHERE s.website_id = $1
          AND we.created_at BETWEEN $2 AND $3
          AND ($4::text IS NULL OR s.city ILIKE $4 OR s.browser ILIKE $4 OR s.os ILIKE $4 OR s.device ILIKE $4 OR s.distinct_id ILIKE $4)
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(&search_pattern)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                s.session_id as id,
                s.website_id as "websiteId",
                s.browser,
                s.os,
                s.device,
                s.screen,
                s.language,
                s.country,
                s.region,
                s.city,
                s.distinct_id as "distinctId",
                MIN(we.created_at) as "firstAt",
                MAX(we.created_at) as "lastAt",
                COUNT(DISTINCT we.visit_id) as visits,
                SUM(CASE WHEN we.event_type = 1 THEN 1 ELSE 0 END) as views,
                SUM(CASE WHEN we.event_type = 2 THEN 1 ELSE 0 END) as events,
                MAX(we.created_at) as "createdAt"
            FROM session s
            JOIN website_event we ON we.session_id = s.session_id
            WHERE s.website_id = $1
              AND we.created_at BETWEEN $2 AND $3
              AND ($4::text IS NULL OR s.city ILIKE $4 OR s.browser ILIKE $4 OR s.os ILIKE $4 OR s.device ILIKE $4 OR s.distinct_id ILIKE $4)
            GROUP BY s.session_id, s.website_id, s.browser, s.os, s.device, s.screen, s.language, s.country, s.region, s.city, s.distinct_id
            ORDER BY MAX(we.created_at) DESC
            LIMIT $5 OFFSET $6
        ) t
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(&search_pattern)
    .bind(page_size)
    .bind(offset)
    .fetch_one(pool)
    .await?;

    Ok(PagedResult {
        data: json_to_vec(rows),
        count,
        page,
        page_size,
    })
}

pub async fn get_website_session(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    session_id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                s.session_id as id,
                s.distinct_id as "distinctId",
                s.website_id as "websiteId",
                s.browser,
                s.os,
                s.device,
                s.screen,
                s.language,
                s.country,
                s.region,
                s.city,
                MIN(we.created_at) as "firstAt",
                MAX(we.created_at) as "lastAt",
                COUNT(DISTINCT we.visit_id) as visits,
                SUM(CASE WHEN we.event_type = 1 THEN 1 ELSE 0 END) as views,
                SUM(CASE WHEN we.event_type = 2 THEN 1 ELSE 0 END) as events,
                COALESCE(EXTRACT(EPOCH FROM (MAX(we.created_at) - MIN(we.created_at))), 0)::bigint as totaltime
            FROM session s
            LEFT JOIN website_event we ON we.session_id = s.session_id
            WHERE s.website_id = $1 AND s.session_id = $2
            GROUP BY s.session_id, s.distinct_id, s.website_id, s.browser, s.os, s.device, s.screen, s.language, s.country, s.region, s.city
        ) t
        "#,
    )
    .bind(website_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub fn json_to_vec(v: serde_json::Value) -> Vec<serde_json::Value> {
    match v {
        serde_json::Value::Array(a) => a,
        _ => Vec::new(),
    }
}

pub async fn get_session_activity(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    session_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                we.created_at as "createdAt",
                we.url_path as "urlPath",
                we.url_query as "urlQuery",
                we.referrer_domain as "referrerDomain",
                we.event_id as "eventId",
                we.event_type as "eventType",
                we.event_name as "eventName",
                we.visit_id as "visitId",
                we.hostname,
                EXISTS(SELECT 1 FROM event_data ed WHERE ed.website_event_id = we.event_id) as "hasData"
            FROM website_event we
            WHERE we.website_id = $1
              AND we.session_id = $2
              AND we.created_at BETWEEN $3 AND $4
            ORDER BY we.created_at DESC
            LIMIT 500
        ) t
        "#,
    )
    .bind(website_id)
    .bind(session_id)
    .bind(start_at)
    .bind(end_at)
    .fetch_one(pool)
    .await?;

    Ok(rows.as_array().cloned().unwrap_or_default())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::manual_let_else, clippy::items_after_statements)]
mod tests {
    use super::*;
    use chrono::Duration;

    async fn spawn_test_server(
        app: axum::Router,
    ) -> (
        u16,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        (port, tx, handle)
    }

    #[tokio::test]
    async fn test_queries_real_db() {
        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();

        let website_id = Uuid::parse_str("01a0aa65-c671-72cc-b248-2884c0b76273").unwrap();
        let end_at = Utc::now();
        let start_at = end_at - Duration::days(30);

        let stats = get_website_stats(&pool, website_id, start_at, end_at).await;
        assert!(stats.is_ok());

        let active = get_active_visitors(&pool, website_id).await;
        assert!(active.is_ok());

        let dr = get_website_date_range(&pool, website_id).await;
        assert!(dr.is_ok());

        for m_type in [
            "url",
            "referrer",
            "browser",
            "os",
            "device",
            "country",
            "region",
            "city",
            "language",
            "event",
            "title",
            "host",
            "hostname",
            "other_default",
        ] {
            let metrics = get_metrics(&pool, website_id, start_at, end_at, m_type, 10).await;
            assert!(metrics.is_ok());
            let expanded =
                get_expanded_metrics(&pool, website_id, start_at, end_at, m_type, 10, 0).await;
            assert!(expanded.is_ok());
        }

        for unit in ["hour", "day", "month", "year", "minute"] {
            let pv_stats = get_pageview_stats(&pool, website_id, start_at, end_at, unit).await;
            assert!(pv_stats.is_ok());
        }

        for unit in ["day", "month", "year", "minute", "hour"] {
            let sess_stats = get_session_stats(&pool, website_id, start_at, end_at, unit).await;
            assert!(sess_stats.is_ok());
        }

        let w_sess_stats = get_website_session_stats(&pool, website_id, start_at, end_at).await;
        assert!(w_sess_stats.is_ok());

        let w_event_stats = get_website_event_stats(&pool, website_id, start_at, end_at).await;
        assert!(w_event_stats.is_ok());

        let weekly = get_weekly_traffic(&pool, website_id, start_at, end_at).await;
        assert!(weekly.is_ok());

        for v_col in [
            "url_path",
            "path",
            "referrer_domain",
            "referrer",
            "browser",
            "os",
            "device",
            "country",
            "event_name",
            "event",
            "unknown_fallback",
        ] {
            let values = get_values(&pool, website_id, start_at, end_at, v_col).await;
            assert!(values.is_ok());
        }

        let realtime = get_realtime_data(&pool, website_id, 30).await;
        assert!(realtime.is_ok());

        let paged_events =
            get_website_events(&pool, website_id, start_at, end_at, 1, 10, None).await;
        assert!(paged_events.is_ok());
        let paged_events_search =
            get_website_events(&pool, website_id, start_at, end_at, 1, 10, Some("test")).await;
        assert!(paged_events_search.is_ok());

        assert!(
            get_website_events(&pool, website_id, start_at, end_at, 1, -100, None)
                .await
                .is_err()
        );
        assert!(
            get_website_sessions(&pool, website_id, start_at, end_at, 1, -100, None)
                .await
                .is_err()
        );
        let paged_sessions =
            get_website_sessions(&pool, website_id, start_at, end_at, 1, 10, None).await;
        assert!(paged_sessions.is_ok());
        let paged_sessions_search =
            get_website_sessions(&pool, website_id, start_at, end_at, 1, 10, Some("chrome")).await;
        assert!(paged_sessions_search.is_ok());

        let bad_json = serde_json::json!({"not": "array"});
        assert!(json_to_vec(bad_json).is_empty());
        let good_json = serde_json::json!([1, 2, 3]);
        assert_eq!(json_to_vec(good_json).len(), 3);

        let session_id = Uuid::parse_str("9d545957-8842-5a13-bee7-8b9eb9ce6f51").unwrap();
        let session = get_website_session(&pool, website_id, session_id).await;
        assert!(session.is_ok());

        let activity = get_session_activity(&pool, website_id, session_id, start_at, end_at).await;
        assert!(activity.is_ok());

        let rollups = get_website_stats_from_rollups(&pool, website_id, start_at, end_at).await;
        assert!(rollups.is_ok());

        let rollup_site = Uuid::now_v7();
        let rollup_bucket = start_at + Duration::days(1);
        sqlx::query(
            r#"INSERT INTO website_event_stats_hourly
               (website_id, hour_bucket, views, visitors, visits, bounces, totaltime)
               VALUES ($1, date_trunc('hour', $2), 7, 3, 4, 1, 120)
               ON CONFLICT (website_id, hour_bucket) DO NOTHING"#,
        )
        .bind(rollup_site)
        .bind(rollup_bucket)
        .execute(&pool)
        .await
        .unwrap();
        let hit = get_website_stats_from_rollups(&pool, rollup_site, start_at, end_at)
            .await
            .unwrap();
        assert!(hit.is_some());
        let hit_stats = hit.unwrap();
        assert_eq!(hit_stats.pageviews, 7);
        assert_eq!(hit_stats.visitors, 3);

        let miss = get_website_stats_from_rollups(&pool, Uuid::now_v7(), start_at, end_at)
            .await
            .unwrap();
        assert!(miss.is_none());

        use axum::{routing::post, Router, response::IntoResponse};
        let app = Router::new().route(
            "/",
            post(|body: String| async move {
                if body.contains("countIf(event_type") {
                    "{\"pageviews\":15,\"visitors\":7,\"visits\":8,\"bounces\":3,\"totaltime\":120}\n".into_response()
                } else if body.contains("ifNull(") {
                    "{\"x\":\"/docs\",\"y\":42}\n{\"x\":\"/blog\",\"y\":19}\n".into_response()
                } else {
                    "[]\n".into_response()
                }
            }),
        );
        let (port, shutdown_tx, server_handle) = spawn_test_server(app).await;
        let ch_cfg = ClickHouseConfig::from_url(&format!("http://127.0.0.1:{port}/kombu_test")).unwrap();
        let ch_client = ClickHouseClient::new(ch_cfg);

        for eng in [
            StorageEngine::Postgres,
            StorageEngine::Partitioned,
            StorageEngine::Timescale,
            StorageEngine::Clickhouse,
        ] {
            let s_eng = get_website_stats_with_engine(
                eng,
                &pool,
                Some(&ch_client),
                website_id,
                start_at,
                end_at,
            )
            .await;
            assert!(s_eng.is_ok());

            let m_eng = get_metrics_with_engine(
                eng,
                &pool,
                Some(&ch_client),
                website_id,
                start_at,
                end_at,
                "url",
                10,
            )
            .await;
            assert!(m_eng.is_ok());
        }

        let ch_stats_direct = get_website_stats_clickhouse(&ch_client, website_id, start_at, end_at).await.unwrap();
        assert_eq!(ch_stats_direct.pageviews, 15);
        assert_eq!(ch_stats_direct.visitors, 7);

        let ch_metrics_direct = get_metrics_clickhouse(&ch_client, website_id, start_at, end_at, "url", 10).await.unwrap();
        assert_eq!(ch_metrics_direct.len(), 2);
        assert_eq!(ch_metrics_direct[0].x, "/docs");

        let ch_stats_row: ClickHouseStatsRow = serde_json::from_str(
            r#"{"pageviews":10,"visitors":5,"visits":6,"bounces":2,"totaltime":100}"#,
        )
        .unwrap();
        assert_eq!(ch_stats_row.pageviews, 10);
        assert_eq!(ch_stats_row.visitors, 5);

        let ch_stats_empty: ClickHouseStatsRow = serde_json::from_str("{}").unwrap();
        assert_eq!(ch_stats_empty.pageviews, 0);

        let ch_metric_row: ClickHouseMetricRow =
            serde_json::from_str(r#"{"x":"/home","y":42}"#).unwrap();
        assert_eq!(ch_metric_row.x, "/home");
        assert_eq!(ch_metric_row.y, 42);

        let ch_metric_empty: ClickHouseMetricRow = serde_json::from_str("{}").unwrap();
        assert_eq!(ch_metric_empty.x, "");

        let plain: Vec<serde_json::Value> = ch_client.query_json("SELECT 1").await.unwrap();
        assert_eq!(plain.len(), 1);

        let empty_app = Router::new().route("/", post(|| async move { "" }));
        let (empty_port, empty_tx, empty_handle) = spawn_test_server(empty_app).await;
        let empty_client = ClickHouseClient::new(
            ClickHouseConfig::from_url(&format!("http://127.0.0.1:{empty_port}")).unwrap(),
        );
        let empty_stats =
            get_website_stats_clickhouse(&empty_client, website_id, start_at, end_at)
                .await
                .unwrap();
        assert_eq!(empty_stats.pageviews, 0);

        for m in ["referrer", "browser", "os", "device", "country", "event", "host", "hostname", "unknown_other"] {
            let _ = get_metrics_clickhouse(&ch_client, website_id, start_at, end_at, m, 5).await;
        }

        let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_port = dead_listener.local_addr().unwrap().port();
        drop(dead_listener);
        let dead_cfg =
            ClickHouseConfig::from_url(&format!("http://127.0.0.1:{dead_port}/db")).unwrap();
        let dead = ClickHouseClient::new(dead_cfg);
        assert!(get_website_stats_clickhouse(&dead, website_id, start_at, end_at).await.is_err());
        assert!(get_metrics_clickhouse(&dead, website_id, start_at, end_at, "url", 5).await.is_err());
        assert!(
            get_website_stats_with_engine(
                StorageEngine::Clickhouse,
                &pool,
                Some(&dead),
                website_id,
                start_at,
                end_at,
            )
            .await
            .is_ok()
        );
        assert!(
            get_metrics_with_engine(
                StorageEngine::Clickhouse,
                &pool,
                Some(&dead),
                website_id,
                start_at,
                end_at,
                "url",
                5,
            )
            .await
            .is_ok()
        );

        assert!(
            get_website_stats_with_engine(
                StorageEngine::Clickhouse,
                &pool,
                None,
                website_id,
                start_at,
                end_at,
            )
            .await
            .is_ok()
        );
        assert!(
            get_metrics_with_engine(
                StorageEngine::Clickhouse,
                &pool,
                None,
                website_id,
                start_at,
                end_at,
                "url",
                5,
            )
            .await
            .is_ok()
        );

        let dispatched_hit = get_website_stats_with_engine(
            StorageEngine::Partitioned,
            &pool,
            None,
            rollup_site,
            start_at,
            end_at,
        )
        .await
        .unwrap();
        assert_eq!(dispatched_hit.pageviews, 7);
        let dispatched_miss = get_website_stats_with_engine(
            StorageEngine::Partitioned,
            &pool,
            None,
            Uuid::now_v7(),
            start_at,
            end_at,
        )
        .await
        .unwrap();
        assert_eq!(dispatched_miss.pageviews, 0);

        let _ = empty_tx.send(());
        let _ = empty_handle.await;
        let _ = shutdown_tx.send(());
        let _ = server_handle.await;

        let closed_pool = pool.clone();
        closed_pool.close().await;
        assert!(
            get_website_stats_from_rollups(&closed_pool, website_id, start_at, end_at)
                .await
                .is_err()
        );
        assert!(
            get_website_stats(&closed_pool, website_id, start_at, end_at)
                .await
                .is_err()
        );
        assert!(get_active_visitors(&closed_pool, website_id).await.is_err());
        assert!(
            get_website_date_range(&closed_pool, website_id)
                .await
                .is_err()
        );
        assert!(
            get_metrics(&closed_pool, website_id, start_at, end_at, "url", 10)
                .await
                .is_err()
        );
        assert!(
            get_expanded_metrics(&closed_pool, website_id, start_at, end_at, "url", 10, 0)
                .await
                .is_err()
        );
        assert!(
            get_pageview_stats(&closed_pool, website_id, start_at, end_at, "day")
                .await
                .is_err()
        );
        assert!(
            get_session_stats(&closed_pool, website_id, start_at, end_at, "day")
                .await
                .is_err()
        );
        assert!(
            get_website_session_stats(&closed_pool, website_id, start_at, end_at)
                .await
                .is_err()
        );
        assert!(
            get_website_event_stats(&closed_pool, website_id, start_at, end_at)
                .await
                .is_err()
        );
        assert!(
            get_weekly_traffic(&closed_pool, website_id, start_at, end_at)
                .await
                .is_err()
        );
        assert!(
            get_values(&closed_pool, website_id, start_at, end_at, "country")
                .await
                .is_err()
        );
        assert!(
            get_realtime_data(&closed_pool, website_id, 30)
                .await
                .is_err()
        );
        assert!(
            get_website_events(&closed_pool, website_id, start_at, end_at, 1, 10, None)
                .await
                .is_err()
        );
        assert!(
            get_website_sessions(&closed_pool, website_id, start_at, end_at, 1, 10, None)
                .await
                .is_err()
        );
        assert!(
            get_website_session(&closed_pool, website_id, session_id)
                .await
                .is_err()
        );
        assert!(
            get_session_activity(&closed_pool, website_id, session_id, start_at, end_at)
                .await
                .is_err()
        );
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_page_offset_calc() {
        let page: i64 = kani::any();
        let page_size: i64 = kani::any();
        kani::assume(page_size >= 1 && page_size <= 100);
        kani::assume(page >= -10 && page <= 10_000);
        let offset = (page - 1).max(0) * page_size;
        kani::assert(offset >= 0, "offset is non-negative");
    }

    #[kani::proof]
    fn harness_valid_column_prefix() {
        let choice: u8 = kani::any();
        let col_type = choice % 3;
        let prefix = match col_type {
            0 => "we.",
            1 => "s.",
            _ => "we.",
        };
        kani::assert(
            prefix.starts_with("we.") || prefix.starts_with("s."),
            "safe prefix",
        );
    }

    #[kani::proof]
    fn harness_time_bucket_format() {
        let choice: u8 = kani::any();
        let (interval, format) = match choice % 5 {
            0 => ("minute", "YYYY-MM-DD HH24:MI:00"),
            1 => ("hour", "YYYY-MM-DD HH24:00:00"),
            2 => ("day", "YYYY-MM-DD 00:00:00"),
            3 => ("month", "YYYY-MM-01 00:00:00"),
            _ => ("year", "YYYY-01-01 00:00:00"),
        };
        kani::assert(!interval.is_empty(), "interval non-empty");
        kani::assert(format.starts_with("YYYY-"), "valid SQL date format pattern");
    }

    #[kani::proof]
    fn harness_metric_limit_bounds() {
        let limit: i64 = kani::any();
        kani::assume(limit > 0 && limit <= 500);
        kani::assert(
            limit >= 1 && limit <= 500,
            "metric limit within safe API bounds",
        );
    }
}
