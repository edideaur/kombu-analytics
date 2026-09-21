#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::router::AppState;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;
const DEFAULT_LOOKBACK_DAYS: i64 = 30;

#[derive(Debug, Clone, Deserialize)]
pub struct ReportsQuery {
    #[serde(rename = "websiteId")]
    pub website_id: Option<Uuid>,
    pub r#type: Option<String>,
    pub search: Option<String>,
}

pub async fn list(
    Query(query): Query<ReportsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                report_id as id,
                user_id as "userId",
                website_id as "websiteId",
                type,
                name,
                description,
                parameters,
                created_at as "createdAt",
                updated_at as "updatedAt"
            FROM "report"
            WHERE ($1::uuid IS NULL OR website_id = $1)
              AND ($2::text IS NULL OR type = $2)
              AND ($3::text IS NULL OR name ILIKE $3)
            ORDER BY created_at DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(query.website_id)
    .bind(query.r#type)
    .bind(query.search.as_ref().map(|s| format!("%{s}%")))
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

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
    Query(query): Query<ReportsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut q = query;
    q.website_id = Some(website_id);
    list(Query(q), State(state)).await
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let report_id = Uuid::now_v7();
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "websiteId required" })),
            )
        })?;
    let user_id = body["userId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::now_v7);
    let r#type = body["type"].as_str().unwrap_or("funnel");
    let name = body["name"].as_str().unwrap_or("Untitled Report");
    let description = body["description"].as_str().unwrap_or("");
    let parameters = body["parameters"].clone();

    sqlx::query(
        r#"
        INSERT INTO "report" (report_id, user_id, website_id, type, name, description, parameters, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        "#,
    )
    .bind(report_id)
    .bind(user_id)
    .bind(website_id)
    .bind(r#type)
    .bind(name)
    .bind(description)
    .bind(parameters)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(report_id), State(state)).await
}

pub async fn get(
    Path(report_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                report_id as id,
                user_id as "userId",
                website_id as "websiteId",
                type,
                name,
                description,
                parameters,
                created_at as "createdAt",
                updated_at as "updatedAt"
            FROM "report"
            WHERE report_id = $1
        ) t
        "#,
    )
    .bind(report_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some(r) => Ok(Json(r)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Report not found" })),
        )),
    }
}

pub async fn update(
    Path(report_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str();
    let description = body["description"].as_str();
    let parameters = body.get("parameters").cloned();

    sqlx::query(
        r#"
        UPDATE "report"
        SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            parameters = COALESCE($4, parameters),
            updated_at = NOW()
        WHERE report_id = $1
        "#,
    )
    .bind(report_id)
    .bind(name)
    .bind(description)
    .bind(parameters)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(report_id), State(state)).await
}

pub async fn delete(
    Path(report_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(r#"DELETE FROM "report" WHERE report_id = $1"#)
        .bind(report_id)
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

pub async fn run_funnel(Json(body): Json<Value>) -> Json<Value> {
    let steps = body["steps"].as_array().map_or(0, |a| a.len());
    Json(json!({
        "steps": steps,
        "conversion": []
    }))
}

pub async fn run_journey(Json(_body): Json<Value>) -> Json<Value> {
    Json(json!([]))
}

pub async fn run_retention(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let days = body["days"].as_i64().unwrap_or(30).clamp(7, 90);

    let cohort_rows = sqlx::query_as::<_, (chrono::NaiveDate, i64, i64)>(
        r#"
        WITH first_visits AS (
            SELECT
                session_id,
                DATE(MIN(created_at)) AS cohort_date
            FROM website_event
            WHERE website_id = $1
              AND is_bot = false
              AND created_at >= NOW() - ($2 || ' days')::interval
            GROUP BY session_id
        ),
        activity AS (
            SELECT
                e.session_id,
                f.cohort_date,
                (DATE(e.created_at) - f.cohort_date)::bigint AS day_offset
            FROM website_event e
            JOIN first_visits f ON e.session_id = f.session_id
            WHERE e.website_id = $1
              AND e.is_bot = false
              AND e.created_at >= NOW() - ($2 || ' days')::interval
            GROUP BY e.session_id, f.cohort_date, day_offset
        )
        SELECT
            cohort_date,
            day_offset,
            COUNT(DISTINCT session_id)::bigint AS visitors
        FROM activity
        WHERE day_offset >= 0 AND day_offset <= 30
        GROUP BY cohort_date, day_offset
        ORDER BY cohort_date DESC, day_offset ASC
        LIMIT 500
        "#,
    )
    .bind(website_id)
    .bind(days.to_string())
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut matrix: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    let mut cohort_sizes: BTreeMap<String, i64> = BTreeMap::new();

    for (cohort_date, day_offset, visitors) in cohort_rows {
        let date_str = cohort_date.to_string();
        if day_offset == 0 {
            cohort_sizes.insert(date_str.clone(), visitors);
        }
        matrix
            .entry(date_str)
            .or_default()
            .insert(format!("day_{day_offset}"), visitors);
    }

    let cohorts: Vec<Value> = matrix
        .into_iter()
        .map(|(date, days_map)| {
            let total = *cohort_sizes.get(&date).unwrap_or(&0);
            json!({
                "date": date,
                "cohortSize": total,
                "retention": days_map
            })
        })
        .collect();

    Ok(Json(json!({
        "websiteId": website_id,
        "days": days,
        "cohorts": cohorts
    })))
}

pub async fn run_attribution(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let model = body["model"].as_str().unwrap_or("all");
    let target_event = body["targetEvent"].as_str();

    let rows = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>, bool)>(
        r#"
        SELECT
            session_id,
            COALESCE(
                utm_source,
                referrer_domain,
                CASE
                    WHEN gclid IS NOT NULL THEN 'Google Ads'
                    WHEN fbclid IS NOT NULL THEN 'Meta Ads'
                    WHEN msclkid IS NOT NULL THEN 'Microsoft Ads'
                    WHEN ttclid IS NOT NULL THEN 'TikTok Ads'
                    WHEN li_fat_id IS NOT NULL THEN 'LinkedIn Ads'
                    WHEN twclid IS NOT NULL THEN 'Twitter/X Ads'
                    ELSE 'Direct'
                END
            ) as channel,
            created_at,
            COALESCE((event_type = 2 OR ($2::text IS NOT NULL AND event_name = $2)), false) as is_conversion
        FROM website_event
        WHERE website_id = $1
          AND is_bot = false
          AND created_at >= NOW() - INTERVAL '30 days'
        ORDER BY session_id, created_at ASC
        LIMIT 10000
        "#,
    )
    .bind(website_id)
    .bind(target_event)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut session_events: HashMap<Uuid, Vec<(String, chrono::DateTime<chrono::Utc>, bool)>> =
        HashMap::new();
    for (sid, channel, created_at, is_conv) in rows {
        session_events
            .entry(sid)
            .or_default()
            .push((channel, created_at, is_conv));
    }

    let mut first_touch: HashMap<String, f64> = HashMap::new();
    let mut last_touch: HashMap<String, f64> = HashMap::new();
    let mut linear: HashMap<String, f64> = HashMap::new();
    let mut total_conversions = 0.0;

    for (_sid, events) in session_events {
        let has_conv = events.iter().any(|(_, _, c)| *c);
        if !has_conv {
            continue;
        }
        total_conversions += 1.0;

        let touches: Vec<&str> = events.iter().map(|(ch, _, _)| ch.as_str()).collect();

        let first = touches[0];
        let last = touches[touches.len() - 1];

        *first_touch.entry(first.to_string()).or_insert(0.0) += 1.0;
        *last_touch.entry(last.to_string()).or_insert(0.0) += 1.0;

        let weight = 1.0 / touches.len() as f64;
        for t in touches {
            *linear.entry(t.to_string()).or_insert(0.0) += weight;
        }
    }

    Ok(Json(json!({
        "websiteId": website_id,
        "model": model,
        "totalConversions": total_conversions,
        "firstTouch": first_touch,
        "lastTouch": last_touch,
        "linear": linear
    })))
}

pub async fn run_revenue(Json(_body): Json<Value>) -> Json<Value> {
    Json(json!([]))
}

pub async fn run_goal(Json(_body): Json<Value>) -> Json<Value> {
    Json(json!([]))
}

pub async fn run_utm(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let query_param = |col: &str| {
        format!(
            r#"
            SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
                SELECT {col} as "utm", count(*)::bigint as "views"
                FROM website_event
                WHERE website_id = $1 AND {col} IS NOT NULL
                GROUP BY {col}
                ORDER BY views DESC
                LIMIT 50
            ) t
            "#
        )
    };

    let utm_source = sqlx::query_scalar::<_, serde_json::Value>(&query_param("utm_source"))
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let utm_medium = sqlx::query_scalar::<_, serde_json::Value>(&query_param("utm_medium"))
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let utm_campaign = sqlx::query_scalar::<_, serde_json::Value>(&query_param("utm_campaign"))
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let utm_term = sqlx::query_scalar::<_, serde_json::Value>(&query_param("utm_term"))
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let utm_content = sqlx::query_scalar::<_, serde_json::Value>(&query_param("utm_content"))
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let gclid_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*)::bigint FROM website_event WHERE website_id = $1 AND gclid IS NOT NULL"#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let fbclid_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*)::bigint FROM website_event WHERE website_id = $1 AND fbclid IS NOT NULL"#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let msclkid_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*)::bigint FROM website_event WHERE website_id = $1 AND msclkid IS NOT NULL"#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let ttclid_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*)::bigint FROM website_event WHERE website_id = $1 AND ttclid IS NOT NULL"#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let li_fat_id_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*)::bigint FROM website_event WHERE website_id = $1 AND li_fat_id IS NOT NULL"#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let twclid_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*)::bigint FROM website_event WHERE website_id = $1 AND twclid IS NOT NULL"#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let ad_clicks = json!([
        { "platform": "Google Ads", "param": "gclid", "clicks": gclid_count },
        { "platform": "Meta Ads", "param": "fbclid", "clicks": fbclid_count },
        { "platform": "Microsoft Ads", "param": "msclkid", "clicks": msclkid_count },
        { "platform": "TikTok Ads", "param": "ttclid", "clicks": ttclid_count },
        { "platform": "LinkedIn Ads", "param": "li_fat_id", "clicks": li_fat_id_count },
        { "platform": "Twitter/X Ads", "param": "twclid", "clicks": twclid_count },
    ]);

    Ok(Json(json!({
        "utm_source": utm_source,
        "utm_medium": utm_medium,
        "utm_campaign": utm_campaign,
        "utm_term": utm_term,
        "utm_content": utm_content,
        "ad_clicks": ad_clicks
    })))
}

pub async fn run_performance(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let metric = body["parameters"]["metric"].as_str().unwrap_or("lcp");
    let metric_col = match metric {
        "lcp" => "lcp",
        "cls" => "cls",
        "inp" => "inp",
        "fcp" => "fcp",
        "ttfb" => "ttfb",
        _ => "lcp",
    };

    let sql = format!(
        r#"
        SELECT
            COALESCE(jsonb_agg(t), '[]'::jsonb)
        FROM (
            SELECT
                TO_CHAR(DATE_TRUNC('hour', created_at), 'YYYY-MM-DD HH24:00:00') as x,
                COALESCE(AVG({metric_col}), 0)::float as y
            FROM website_event
            WHERE website_id = $1 AND {metric_col} IS NOT NULL
            GROUP BY 1
            ORDER BY 1 ASC
        ) t
        "#
    );

    let chart_rows = sqlx::query_scalar::<_, serde_json::Value>(&sql)
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let summary_sql = format!(
        r#"
        SELECT
            COUNT(*)::bigint,
            COALESCE(MIN({metric_col}), 0)::float,
            COALESCE(MAX({metric_col}), 0)::float,
            COALESCE(AVG({metric_col}), 0)::float,
            COALESCE(percentile_cont(0.75) WITHIN GROUP (ORDER BY {metric_col}), 0)::float,
            COALESCE(percentile_cont(0.90) WITHIN GROUP (ORDER BY {metric_col}), 0)::float,
            COALESCE(percentile_cont(0.99) WITHIN GROUP (ORDER BY {metric_col}), 0)::float
        FROM website_event
        WHERE website_id = $1 AND {metric_col} IS NOT NULL
        "#
    );

    let summary = if let Ok(row) =
        sqlx::query_as::<_, (i64, f64, f64, f64, f64, f64, f64)>(&summary_sql)
            .bind(website_id)
            .fetch_one(&state.pool)
            .await
    {
        json!({
            "count": row.0,
            "min": row.1,
            "max": row.2,
            "avg": row.3,
            "p75": row.4,
            "p90": row.5,
            "p99": row.6
        })
    } else {
        json!({
            "count": 0, "min": 0, "max": 0, "avg": 0, "p75": 0, "p90": 0, "p99": 0
        })
    };

    let pages_sql = format!(
        r#"
        SELECT COALESCE(jsonb_agg(p), '[]'::jsonb) FROM (
            SELECT
                url_path as x,
                COUNT(*)::bigint as count,
                COALESCE(percentile_cont(0.75) WITHIN GROUP (ORDER BY {metric_col}), 0)::float as p75,
                COALESCE(AVG({metric_col}), 0)::float as avg
            FROM website_event
            WHERE website_id = $1 AND {metric_col} IS NOT NULL
            GROUP BY url_path
            ORDER BY count DESC
            LIMIT 20
        ) p
        "#
    );
    let pages = sqlx::query_scalar::<_, serde_json::Value>(&pages_sql)
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let page_titles_sql = format!(
        r#"
        SELECT COALESCE(jsonb_agg(pt), '[]'::jsonb) FROM (
            SELECT
                COALESCE(page_title, 'None') as x,
                COUNT(*)::bigint as count,
                COALESCE(percentile_cont(0.75) WITHIN GROUP (ORDER BY {metric_col}), 0)::float as p75,
                COALESCE(AVG({metric_col}), 0)::float as avg
            FROM website_event
            WHERE website_id = $1 AND {metric_col} IS NOT NULL
            GROUP BY page_title
            ORDER BY count DESC
            LIMIT 20
        ) pt
        "#
    );
    let page_titles = sqlx::query_scalar::<_, serde_json::Value>(&page_titles_sql)
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let devices_sql = format!(
        r#"
        SELECT COALESCE(jsonb_agg(d), '[]'::jsonb) FROM (
            SELECT
                COALESCE(s.device, 'unknown') as x,
                COUNT(*)::bigint as count,
                COALESCE(percentile_cont(0.75) WITHIN GROUP (ORDER BY {metric_col}), 0)::float as p75,
                COALESCE(AVG({metric_col}), 0)::float as avg
            FROM website_event e
            JOIN session s ON e.session_id = s.session_id
            WHERE e.website_id = $1 AND e.{metric_col} IS NOT NULL
            GROUP BY s.device
            ORDER BY count DESC
            LIMIT 10
        ) d
        "#
    );
    let devices = sqlx::query_scalar::<_, serde_json::Value>(&devices_sql)
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    let browsers_sql = format!(
        r#"
        SELECT COALESCE(jsonb_agg(b), '[]'::jsonb) FROM (
            SELECT
                COALESCE(s.browser, 'unknown') as x,
                COUNT(*)::bigint as count,
                COALESCE(percentile_cont(0.75) WITHIN GROUP (ORDER BY {metric_col}), 0)::float as p75,
                COALESCE(AVG({metric_col}), 0)::float as avg
            FROM website_event e
            JOIN session s ON e.session_id = s.session_id
            WHERE e.website_id = $1 AND e.{metric_col} IS NOT NULL
            GROUP BY s.browser
            ORDER BY count DESC
            LIMIT 10
        ) b
        "#
    );
    let browsers = sqlx::query_scalar::<_, serde_json::Value>(&browsers_sql)
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    Ok(Json(json!({
        "chart": chart_rows,
        "summary": summary,
        "pages": pages,
        "pageTitles": page_titles,
        "devices": devices,
        "browsers": browsers
    })))
}

pub async fn run_errors(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let sql = r#"
        SELECT
            COALESCE(jsonb_agg(t), '[]'::jsonb)
        FROM (
            SELECT
                event_name as message,
                url_path as path,
                COUNT(*)::bigint as count,
                COUNT(DISTINCT session_id)::bigint as users,
                MAX(created_at) as last_seen
            FROM website_event
            WHERE website_id = $1 AND event_type = 6
            GROUP BY event_name, url_path
            ORDER BY count DESC
            LIMIT 50
        ) t
    "#;

    let error_rows = sqlx::query_scalar::<_, serde_json::Value>(sql)
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(json!([]));

    Ok(Json(json!({
        "errors": error_rows,
        "count": error_rows.as_array().map_or(0, |a| a.len())
    })))
}

pub async fn run_breakdown(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                url_path as "path",
                count(*)::bigint as "views",
                count(distinct session_id)::bigint as "visitors",
                count(distinct visit_id)::bigint as "visits",
                0::bigint as "bounces",
                0::bigint as "totaltime"
            FROM website_event
            WHERE website_id = $1
            GROUP BY url_path
            ORDER BY views DESC
            LIMIT 100
        ) t
        "#,
    )
    .bind(website_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(json!([]));

    Ok(Json(rows))
}

pub async fn run_heatmap(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "websiteId required" })),
            )
        })?;
    let url_path = body["urlPath"].as_str().unwrap_or("/");

    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                x, y, page_x as "pageX", page_y as "pageY",
                viewport_w as "viewportW", viewport_h as "viewportH",
                event_type as "eventType", scroll_pct as "scrollPct"
            FROM "heatmap_event"
            WHERE website_id = $1 AND url_path = $2
            LIMIT 1000
        ) t
        "#,
    )
    .bind(website_id)
    .bind(url_path)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(rows))
}

pub async fn run_entry_exit(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let end_at = body["endAt"]
        .as_i64()
        .or_else(|| body["end_at"].as_i64())
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = body["startAt"]
        .as_i64()
        .or_else(|| body["start_at"].as_i64())
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(DEFAULT_LOOKBACK_DAYS));

    let limit = body["limit"]
        .as_i64()
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);

    let entry_rows = sqlx::query_as::<_, (String, i64, i64, f64, i64)>(
        r#"
        WITH session_pages AS (
            SELECT
                session_id,
                visit_id,
                url_path,
                created_at,
                ROW_NUMBER() OVER (PARTITION BY session_id, visit_id ORDER BY created_at ASC) as entry_rank,
                COUNT(*) OVER (PARTITION BY session_id, visit_id) as total_events
            FROM website_event
            WHERE website_id = $1
              AND created_at BETWEEN $2 AND $3
              AND event_type = 1
              AND is_bot = false
        ),
        entry_agg AS (
            SELECT
                url_path,
                COUNT(DISTINCT visit_id)::bigint as entries,
                COUNT(DISTINCT CASE WHEN total_events = 1 THEN visit_id END)::bigint as bounces,
                COUNT(DISTINCT session_id)::bigint as visitors
            FROM session_pages
            WHERE entry_rank = 1
            GROUP BY url_path
            ORDER BY entries DESC
            LIMIT $4
        )
        SELECT
            url_path,
            entries,
            bounces,
            CASE WHEN entries > 0 THEN ROUND((bounces::numeric / entries::numeric) * 100.0, 2)::double precision ELSE 0.0 END as bounce_rate,
            visitors
        FROM entry_agg
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let exit_rows = sqlx::query_as::<_, (String, i64, i64, f64)>(
        r#"
        WITH session_pages AS (
            SELECT
                session_id,
                visit_id,
                url_path,
                created_at,
                ROW_NUMBER() OVER (PARTITION BY session_id, visit_id ORDER BY created_at DESC) as exit_rank
            FROM website_event
            WHERE website_id = $1
              AND created_at BETWEEN $2 AND $3
              AND event_type = 1
              AND is_bot = false
        ),
        pageview_counts AS (
            SELECT
                url_path,
                COUNT(*)::bigint as total_pageviews
            FROM website_event
            WHERE website_id = $1
              AND created_at BETWEEN $2 AND $3
              AND event_type = 1
              AND is_bot = false
            GROUP BY url_path
        ),
        exit_agg AS (
            SELECT
                sp.url_path,
                COUNT(DISTINCT sp.visit_id)::bigint as exits
            FROM session_pages sp
            WHERE sp.exit_rank = 1
            GROUP BY sp.url_path
        )
        SELECT
            e.url_path,
            e.exits,
            COALESCE(p.total_pageviews, e.exits)::bigint as pageviews,
            CASE WHEN COALESCE(p.total_pageviews, e.exits) > 0
                 THEN ROUND((e.exits::numeric / COALESCE(p.total_pageviews, e.exits)::numeric) * 100.0, 2)::double precision
                 ELSE 0.0 END as exit_rate
        FROM exit_agg e
        LEFT JOIN pageview_counts p ON e.url_path = p.url_path
        ORDER BY e.exits DESC
        LIMIT $4
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let entry_pages: Vec<Value> = entry_rows
        .into_iter()
        .map(|(path, entries, bounces, bounce_rate, visitors)| {
            json!({
                "urlPath": path,
                "entries": entries,
                "bounces": bounces,
                "bounceRate": bounce_rate,
                "visitors": visitors,
            })
        })
        .collect();

    let exit_pages: Vec<Value> = exit_rows
        .into_iter()
        .map(|(path, exits, pageviews, exit_rate)| {
            json!({
                "urlPath": path,
                "exits": exits,
                "pageviews": pageviews,
                "exitRate": exit_rate,
            })
        })
        .collect();

    Ok(Json(json!({
        "websiteId": website_id,
        "startAt": start_at,
        "endAt": end_at,
        "entryPages": entry_pages,
        "exitPages": exit_pages,
    })))
}

pub async fn run_engagement(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let website_id = body["websiteId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::nil);

    let end_at = body["endAt"]
        .as_i64()
        .or_else(|| body["end_at"].as_i64())
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = body["startAt"]
        .as_i64()
        .or_else(|| body["start_at"].as_i64())
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(DEFAULT_LOOKBACK_DAYS));

    let url_filter = body["urlPath"]
        .as_str()
        .or_else(|| body["url_path"].as_str())
        .map(String::from);

    let limit = body["limit"]
        .as_i64()
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);

    let totals = sqlx::query_as::<_, (f64, f64, i64, i64, i64, i64, i64, i64)>(
        r#"
        SELECT
            COALESCE(AVG(depth_data.number_value), 0)::double precision as avg_depth,
            COALESCE(AVG(time_data.number_value), 0)::double precision as avg_engaged_time,
            COUNT(DISTINCT we.session_id)::bigint as total_sessions,
            COUNT(DISTINCT we.event_id)::bigint as total_events,
            COALESCE(SUM(CASE WHEN depth_data.number_value >= 25 THEN 1 ELSE 0 END), 0)::bigint as count_25,
            COALESCE(SUM(CASE WHEN depth_data.number_value >= 50 THEN 1 ELSE 0 END), 0)::bigint as count_50,
            COALESCE(SUM(CASE WHEN depth_data.number_value >= 75 THEN 1 ELSE 0 END), 0)::bigint as count_75,
            COALESCE(SUM(CASE WHEN depth_data.number_value >= 100 THEN 1 ELSE 0 END), 0)::bigint as count_100
        FROM website_event we
        LEFT JOIN event_data depth_data
            ON we.event_id = depth_data.website_event_id AND depth_data.data_key IN ('depth', 'scroll_depth', 'sd')
        LEFT JOIN event_data time_data
            ON we.event_id = time_data.website_event_id AND time_data.data_key IN ('engaged_time', 'time', 'e')
        WHERE we.website_id = $1
          AND we.created_at BETWEEN $2 AND $3
          AND we.event_name IN ('scroll', 'Scroll', 'engagement', 'Engagement')
          AND ($4::text IS NULL OR we.url_path = $4)
          AND we.is_bot = false
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(&url_filter)
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0.0, 0.0, 0, 0, 0, 0, 0, 0));

    let page_rows = sqlx::query_as::<_, (String, i64, i64, f64, f64)>(
        r#"
        SELECT
            we.url_path,
            COUNT(DISTINCT we.session_id)::bigint as visitors,
            COUNT(*)::bigint as events,
            COALESCE(AVG(depth_data.number_value), 0)::double precision as avg_depth,
            COALESCE(AVG(time_data.number_value), 0)::double precision as avg_time
        FROM website_event we
        LEFT JOIN event_data depth_data
            ON we.event_id = depth_data.website_event_id AND depth_data.data_key IN ('depth', 'scroll_depth', 'sd')
        LEFT JOIN event_data time_data
            ON we.event_id = time_data.website_event_id AND time_data.data_key IN ('engaged_time', 'time', 'e')
        WHERE we.website_id = $1
          AND we.created_at BETWEEN $2 AND $3
          AND we.event_name IN ('scroll', 'Scroll', 'engagement', 'Engagement')
          AND ($4::text IS NULL OR we.url_path = $4)
          AND we.is_bot = false
        GROUP BY we.url_path
        ORDER BY events DESC
        LIMIT $5
        "#,
    )
    .bind(website_id)
    .bind(start_at)
    .bind(end_at)
    .bind(&url_filter)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let pages: Vec<Value> = page_rows
        .into_iter()
        .map(|(path, visitors, events, avg_depth, avg_time)| {
            json!({
                "urlPath": path,
                "visitors": visitors,
                "events": events,
                "avgScrollDepth": (avg_depth * 100.0).round() / 100.0,
                "avgEngagedSeconds": (avg_time * 10.0).round() / 10.0,
            })
        })
        .collect();

    Ok(Json(json!({
        "websiteId": website_id,
        "startAt": start_at,
        "endAt": end_at,
        "avgScrollDepth": (totals.0 * 100.0).round() / 100.0,
        "avgEngagedSeconds": (totals.1 * 10.0).round() / 10.0,
        "totalSessions": totals.2,
        "totalEvents": totals.3,
        "milestones": {
            "reached25": totals.4,
            "reached50": totals.5,
            "reached75": totals.6,
            "reached100": totals.7,
        },
        "pages": pages,
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reports_error_branches() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let closed_pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        closed_pool.close().await;

        let err_state = AppState {
            pool: closed_pool,
            session_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            website_cache: std::sync::Arc::new(quick_cache::sync::Cache::new(10)),
            rate_limiter: crate::rate_limit::RateLimiter::new(),
            ingest_queue: crate::queue::IngestQueue::new(&pool, 16, 2),
            app_secret: std::sync::Arc::new("kombu-secret".into()),
        };

        let rep_id = Uuid::now_v7();
        let web_id = Uuid::now_v7();

        let q = ReportsQuery {
            website_id: Some(web_id),
            r#type: None,
            search: None,
        };
        assert!(list(Query(q.clone()), State(err_state.clone())).await.is_err());
        assert!(list_for_website(Path(web_id), Query(q), State(err_state.clone())).await.is_err());

        assert!(create(State(err_state.clone()), Json(json!({
            "websiteId": web_id.to_string(),
            "userId": "invalid-uuid",
            "name": "Test Report"
        }))).await.is_err());

        assert!(get(Path(rep_id), State(err_state.clone())).await.is_err());

        assert!(update(Path(rep_id), State(err_state.clone()), Json(json!({
            "name": "Updated Report"
        }))).await.is_err());

        assert!(delete(Path(rep_id), State(err_state.clone())).await.is_err());

        assert!(run_heatmap(State(err_state.clone()), Json(json!({
            "websiteId": web_id.to_string(),
            "urlPath": "/heatmap"
        }))).await.is_err());

        let perf_res = run_performance(State(err_state.clone()), Json(json!({
            "websiteId": web_id.to_string()
        }))).await;
        assert!(perf_res.is_ok());
    }

    #[tokio::test]
    async fn test_attribution_and_retention_conversion_paths() {
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
        let session_id = Uuid::now_v7();
        let visit_id = Uuid::now_v7();
        sqlx::query(r#"INSERT INTO "website" (website_id, name, domain) VALUES ($1, 'Conv Site', 'conv.site')"#)
            .bind(website_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "session" (session_id, website_id) VALUES ($1, $2)"#)
            .bind(session_id)
            .bind(website_id)
            .execute(&pool)
            .await
            .unwrap();
        for (url, age_days, event_type, event_name) in [
            ("/home", 3, 1, None),
            ("/pricing", 0, 1, None),
            ("/signup", 0, 2, Some("signup")),
        ] {
            sqlx::query(
                r#"INSERT INTO "website_event"
                   (event_id, website_id, session_id, visit_id, url_path, event_type, event_name, is_bot, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, false, NOW() - ($8 || ' days')::interval)"#,
            )
            .bind(Uuid::now_v7())
            .bind(website_id)
            .bind(session_id)
            .bind(visit_id)
            .bind(url)
            .bind(event_type)
            .bind(event_name)
            .bind(age_days.to_string())
            .execute(&pool)
            .await
            .unwrap();
        }

        let ret = run_retention(
            State(state.clone()),
            Json(json!({ "websiteId": website_id.to_string(), "days": 30 })),
        )
        .await
        .unwrap();
        assert!(ret.0["cohorts"].as_array().is_some());

        let attr = run_attribution(
            State(state.clone()),
            Json(json!({
                "websiteId": website_id.to_string(),
                "model": "all",
                "targetEvent": "signup"
            })),
        )
        .await
        .unwrap();
        assert_eq!(attr.0["totalConversions"], 1.0);

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
    }
}

