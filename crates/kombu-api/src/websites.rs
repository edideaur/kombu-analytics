#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::ingest::QueryRange;
use crate::router::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartsQuery {
    pub ids: String,
    #[serde(alias = "start_at")]
    pub start_at: Option<i64>,
    #[serde(alias = "end_at")]
    pub end_at: Option<i64>,
    pub timezone: Option<String>,
}

const DEFAULT_PAGE_LIMIT: i64 = 50;
const MAX_PAGE_LIMIT: i64 = 500;
const DEFAULT_ANALYTICS_DAYS: i64 = 30;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageAnalyticsQuery {
    #[serde(alias = "start_at")]
    pub start_at: Option<i64>,
    #[serde(alias = "end_at")]
    pub end_at: Option<i64>,
    pub limit: Option<i64>,
    #[serde(alias = "url_path")]
    pub url_path: Option<String>,
}

pub async fn charts(
    Query(query): Query<ChartsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let ids: Vec<Uuid> = query
        .ids
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();

    let mut result_map = serde_json::Map::new();

    for website_id in ids {
        let count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(DISTINCT session_id)::bigint FROM website_event WHERE website_id = $1"#,
        )
        .bind(website_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

        result_map.insert(
            website_id.to_string(),
            json!({
                "values": [count],
                "total": count
            }),
        );
    }

    Ok(Json(json!({ "data": result_map })))
}

pub async fn list(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = if auth.role == "admin" {
        sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
                SELECT
                    website_id as id,
                    name,
                    domain,
                    user_id as "userId",
                    team_id as "teamId",
                    created_at as "createdAt"
                FROM "website"
                WHERE deleted_at IS NULL
                ORDER BY name ASC
                LIMIT 100
            ) t
            "#,
        )
        .fetch_one(&state.pool)
        .await
    } else {
        sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
                SELECT
                    w.website_id as id,
                    w.name,
                    w.domain,
                    w.user_id as "userId",
                    w.team_id as "teamId",
                    w.created_at as "createdAt"
                FROM "website" w
                LEFT JOIN "team_user" tu ON tu.team_id = w.team_id AND tu.user_id = $1
                WHERE w.deleted_at IS NULL AND (w.user_id = $1 OR tu.user_id IS NOT NULL)
                ORDER BY w.name ASC
                LIMIT 100
            ) t
            "#,
        )
        .bind(auth.user_id)
        .fetch_one(&state.pool)
        .await
    }
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

pub async fn create(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let team_id = body["teamId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let name = body["name"].as_str().unwrap_or("My Website");
    let domain = body["domain"].as_str();
    let website_id = body["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::now_v7);

    let assigned_user_id = if team_id.is_some() {
        None
    } else {
        Some(auth.user_id)
    };

    sqlx::query(
        r#"
        INSERT INTO "website" (website_id, name, domain, user_id, team_id, created_by, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        "#,
    )
    .bind(website_id)
    .bind(name)
    .bind(domain)
    .bind(assigned_user_id)
    .bind(team_id)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "id": website_id,
        "name": name,
        "domain": domain,
        "userId": assigned_user_id,
        "teamId": team_id,
        "createdAt": Utc::now().to_rfc3339()
    })))
}

pub async fn get(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                website_id as id,
                name,
                domain,
                user_id as "userId",
                team_id as "teamId",
                created_at as "createdAt"
            FROM "website"
            WHERE website_id = $1 AND deleted_at IS NULL
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
        Some(w) => Ok(Json(w)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Website not found" })),
        )),
    }
}

pub async fn check_website_access(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    is_admin: bool,
    website_id: Uuid,
) -> bool {
    if is_admin {
        return true;
    }
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM "website" w
            LEFT JOIN "team_user" tu ON tu.team_id = w.team_id AND tu.user_id = $2
            WHERE w.website_id = $1 AND (w.user_id = $2 OR tu.role IN ('owner', 'admin', 'member', 'view-only'))
        )
        "#,
    )
    .bind(website_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

pub async fn check_website_write_access(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    is_admin: bool,
    website_id: Uuid,
) -> bool {
    if is_admin {
        return true;
    }
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM "website" w
            LEFT JOIN "team_user" tu ON tu.team_id = w.team_id AND tu.user_id = $2
            WHERE w.website_id = $1 AND (w.user_id = $2 OR tu.role IN ('owner', 'admin'))
        )
        "#,
    )
    .bind(website_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

pub async fn update(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_write_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    let name = body["name"].as_str();
    let domain = body["domain"].as_str();

    sqlx::query(
        r#"
        UPDATE "website"
        SET
            name = COALESCE($2, name),
            domain = COALESCE($3, domain),
            updated_at = NOW()
        WHERE website_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(domain)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(auth, Path(id), State(state)).await
}

pub async fn delete(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_write_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    sqlx::query(
        r#"
        UPDATE "website"
        SET deleted_at = NOW()
        WHERE website_id = $1
        "#,
    )
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

pub async fn stats(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(params): Query<QueryRange>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let end_at = params
        .end_at
        .and_then(|ts| DateTime::from_timestamp_millis(ts))
        .unwrap_or_else(Utc::now);

    let start_at = params
        .start_at
        .and_then(|ts| DateTime::from_timestamp_millis(ts))
        .unwrap_or_else(|| end_at - Duration::days(1));

    let duration = end_at - start_at;
    let prev_end = start_at;
    let prev_start = prev_end - duration;

    let data = crate::storage::website_stats(&state.pool, id, start_at, end_at)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    let comparison = crate::storage::website_stats(&state.pool, id, prev_start, prev_end)
        .await
        .unwrap_or(kombu_query::WebsiteStats {
            pageviews: 0,
            visitors: 0,
            visits: 0,
            bounces: 0,
            totaltime: 0,
        });

    let res = json!({
        "pageviews": data.pageviews,
        "visitors": data.visitors,
        "visits": data.visits,
        "bounces": data.bounces,
        "totaltime": data.totaltime,
        "comparison": comparison
    });

    Ok(Json(res))
}

pub async fn active(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let count = kombu_query::get_active_visitors(&state.pool, id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!({ "visitors": count, "x": count })))
}

pub async fn daterange(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let range = kombu_query::get_website_date_range(&state.pool, id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!({
        "startDate": range.start_date,
        "endDate": range.end_date
    })))
}

pub async fn metrics(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(params): Query<QueryRange>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let end_at = params
        .end_at
        .and_then(|ts| DateTime::from_timestamp_millis(ts))
        .unwrap_or_else(Utc::now);

    let start_at = params
        .start_at
        .and_then(|ts| DateTime::from_timestamp_millis(ts))
        .unwrap_or_else(|| end_at - Duration::days(1));

    let metric_type = params.r#type.as_deref().unwrap_or("url");
    let limit = params.limit.unwrap_or(10);

    let res =
        crate::storage::website_metrics(&state.pool, id, start_at, end_at, metric_type, limit)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
            })?;

    Ok(Json(serde_json::to_value(res).unwrap_or(json!([]))))
}

pub async fn metrics_expanded(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(params): Query<QueryRange>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let end_at = params
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = params
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(1));

    let metric_type = params.r#type.as_deref().unwrap_or("url");
    let limit = params.limit.unwrap_or(500);
    let offset = params.offset.unwrap_or(0);

    let res = kombu_query::get_expanded_metrics(
        &state.pool,
        id,
        start_at,
        end_at,
        metric_type,
        limit,
        offset,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(serde_json::to_value(res).unwrap_or(json!([]))))
}

pub async fn pageviews(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(params): Query<QueryRange>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let end_at = params
        .end_at
        .and_then(|ts| DateTime::from_timestamp_millis(ts))
        .unwrap_or_else(Utc::now);

    let start_at = params
        .start_at
        .and_then(|ts| DateTime::from_timestamp_millis(ts))
        .unwrap_or_else(|| end_at - Duration::days(1));

    let unit = params.unit.as_deref().unwrap_or("day");

    let pageviews = kombu_query::get_pageview_stats(&state.pool, id, start_at, end_at, unit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    let sessions = kombu_query::get_session_stats(&state.pool, id, start_at, end_at, unit)
        .await
        .unwrap_or_default();

    Ok(Json(json!({
        "pageviews": pageviews,
        "sessions": sessions
    })))
}

pub async fn values(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(params): Query<QueryRange>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let end_at = params
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = params
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(1));

    let val_type = params.r#type.as_deref().unwrap_or("url_path");
    let vals = kombu_query::get_values(&state.pool, id, start_at, end_at, val_type)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!(vals)))
}

pub async fn reset(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_write_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    if let Err(e) = sqlx::query(r#"DELETE FROM "website_event" WHERE website_id = $1"#)
        .bind(id)
        .execute(&state.pool)
        .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ));
    }

    let _ = sqlx::query(r#"DELETE FROM "session" WHERE website_id = $1"#)
        .bind(id)
        .execute(&state.pool)
        .await;

    let _ = sqlx::query(r#"UPDATE "website" SET reset_at = NOW() WHERE website_id = $1"#)
        .bind(id)
        .execute(&state.pool)
        .await;

    Ok(Json(json!({ "ok": true })))
}

pub async fn transfer(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_write_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }

    let user_id = body["userId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    let team_id = body["teamId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());

    if let Some(uid) = user_id {
        sqlx::query(r#"UPDATE "website" SET user_id = $2, team_id = NULL, updated_at = NOW() WHERE website_id = $1"#)
            .bind(id)
            .bind(uid)
            .execute(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    } else if let Some(tid) = team_id {
        sqlx::query(r#"UPDATE "website" SET user_id = NULL, team_id = $2, updated_at = NOW() WHERE website_id = $1"#)
            .bind(id)
            .bind(tid)
            .execute(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    }

    get(auth, Path(id), State(state))
        .await
        .map_err(|(status, _)| {
            (
                status,
                Json(json!({ "error": "Failed to fetch transferred website" })),
            )
        })
}

pub async fn entry_exit(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PageAnalyticsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(DEFAULT_ANALYTICS_DAYS));

    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);

    let body = json!({
        "websiteId": id,
        "startAt": start_at.timestamp_millis(),
        "endAt": end_at.timestamp_millis(),
        "limit": limit
    });

    crate::reports::run_entry_exit(State(state), Json(body)).await
}

pub async fn entry_pages(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PageAnalyticsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let result = entry_exit(auth, Path(id), Query(query), State(state)).await?;
    let data = result.0["entryPages"].clone();
    let count = data.as_array().map_or(0, std::vec::Vec::len);

    Ok(Json(json!({
        "data": data,
        "count": count
    })))
}

pub async fn exit_pages(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PageAnalyticsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let result = entry_exit(auth, Path(id), Query(query), State(state)).await?;
    let data = result.0["exitPages"].clone();
    let count = data.as_array().map_or(0, std::vec::Vec::len);

    Ok(Json(json!({
        "data": data,
        "count": count
    })))
}

pub async fn engagement(
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PageAnalyticsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !check_website_access(&state.pool, auth.user_id, auth.role == "admin", id).await {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Forbidden: insufficient permissions" })),
        ));
    }
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);

    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(DEFAULT_ANALYTICS_DAYS));

    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);

    let body = json!({
        "websiteId": id,
        "startAt": start_at.timestamp_millis(),
        "endAt": end_at.timestamp_millis(),
        "urlPath": query.url_path,
        "limit": limit
    });

    crate::reports::run_engagement(State(state), Json(body)).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_websites_full_lifecycle_and_endpoints() {
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

        let user_id = Uuid::now_v7();
        let auth_user = AuthUser {
            user_id,
            username: "site_owner".into(),
            role: "user".into(),
        };

        let stranger_id = Uuid::now_v7();
        let stranger_auth = AuthUser {
            user_id: stranger_id,
            username: "stranger".into(),
            role: "user".into(),
        };

        let admin_auth = AuthUser {
            user_id: Uuid::now_v7(),
            username: "admin".into(),
            role: "admin".into(),
        };

        for u in [&auth_user, &stranger_auth, &admin_auth] {
            let _ = sqlx::query(
                r#"INSERT INTO "user" (user_id, username, password, role, created_at, updated_at) VALUES ($1, $2, 'pass', $3, NOW(), NOW())"#,
            )
            .bind(u.user_id)
            .bind(&u.username)
            .bind(&u.role)
            .execute(&pool)
            .await;
        }

        let custom_site_id = Uuid::now_v7();
        let res_create = create(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({
                "id": custom_site_id.to_string(),
                "name": "Acme Portal",
                "domain": "acme.portal"
            })),
        )
        .await
        .unwrap();
        let site_id_str = res_create.0["id"].as_str().unwrap();
        let site_id = Uuid::parse_str(site_id_str).unwrap();
        assert_eq!(site_id, custom_site_id);
        assert_eq!(res_create.0["name"], "Acme Portal");

        let test_team_id_init = Uuid::now_v7();
        let _ = sqlx::query(r#"INSERT INTO "team" (team_id, name, access_code) VALUES ($1, 'Init Team', 'code_init')"#).bind(test_team_id_init).execute(&pool).await;
        let res_create_team = create(
            auth_user.clone(),
            State(state.clone()),
            Json(json!({
                "name": "Team Portal",
                "domain": "team.portal",
                "teamId": test_team_id_init.to_string(),
            })),
        )
        .await;
        assert!(res_create_team.is_ok());
        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(Uuid::parse_str(res_create_team.unwrap().0["id"].as_str().unwrap()).unwrap())
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "team" WHERE team_id = $1"#)
            .bind(test_team_id_init)
            .execute(&pool)
            .await;

        let res_list_user = list(auth_user.clone(), State(state.clone())).await.unwrap();
        assert!(res_list_user.0["count"].as_i64().unwrap() >= 1);

        let res_list_admin = list(admin_auth.clone(), State(state.clone()))
            .await
            .unwrap();
        assert!(res_list_admin.0["count"].as_i64().unwrap() >= 1);

        let res_get_owner = get(auth_user.clone(), Path(site_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_get_owner.0["name"], "Acme Portal");

        let res_get_stranger =
            get(stranger_auth.clone(), Path(site_id), State(state.clone())).await;
        assert!(res_get_stranger.is_err());
        assert_eq!(res_get_stranger.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_get_404 = get(
            admin_auth.clone(),
            Path(Uuid::now_v7()),
            State(state.clone()),
        )
        .await;
        assert!(res_get_404.is_err());
        assert_eq!(res_get_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_up = update(
            auth_user.clone(),
            Path(site_id),
            State(state.clone()),
            Json(json!({
                "name": "Acme Mega Portal",
                "domain": "acme-mega.portal"
            })),
        )
        .await
        .unwrap();
        assert_eq!(res_up.0["name"], "Acme Mega Portal");

        let charts_query = ChartsQuery {
            ids: site_id.to_string(),
            start_at: None,
            end_at: None,
            timezone: None,
        };
        let res_charts = charts(Query(charts_query.clone()), State(state.clone()))
            .await
            .unwrap();
        assert!(res_charts.0["data"].is_object());

        let now = Utc::now();
        let qr = QueryRange {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            unit: Some("day".into()),
            r#type: Some("url_path".into()),
            limit: Some(10),
            offset: None,
            search: None,
            timezone: Some("UTC".into()),
            compare: None,
        };

        let _ = stats(
            auth_user.clone(),
            Path(site_id),
            Query(qr.clone()),
            State(state.clone()),
        )
        .await;

        let qr_pv = QueryRange {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            unit: Some("day".into()),
            r#type: None,
            limit: Some(10),
            offset: None,
            search: None,
            timezone: Some("UTC".into()),
            compare: None,
        };
        let _ = pageviews(
            auth_user.clone(),
            Path(site_id),
            Query(qr_pv.clone()),
            State(state.clone()),
        )
        .await;

        let qr_m = QueryRange {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            unit: None,
            r#type: Some("url_path".into()),
            limit: Some(10),
            offset: None,
            search: None,
            timezone: None,
            compare: None,
        };
        let _ = metrics(
            auth_user.clone(),
            Path(site_id),
            Query(qr_m.clone()),
            State(state.clone()),
        )
        .await;
        let _ = metrics_expanded(
            auth_user.clone(),
            Path(site_id),
            Query(qr_m.clone()),
            State(state.clone()),
        )
        .await;

        let qr_m_no_start = QueryRange {
            start_at: None,
            end_at: Some(now.timestamp_millis()),
            unit: None,
            r#type: None,
            limit: None,
            offset: None,
            search: None,
            timezone: None,
            compare: None,
        };
        let _ = metrics_expanded(
            auth_user.clone(),
            Path(site_id),
            Query(qr_m_no_start),
            State(state.clone()),
        )
        .await;

        let _ = active(auth_user.clone(), Path(site_id), State(state.clone())).await;

        let _ = daterange(auth_user.clone(), Path(site_id), State(state.clone())).await;

        let qr_val = QueryRange {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            unit: None,
            r#type: Some("url_path".into()),
            limit: Some(10),
            offset: None,
            search: None,
            timezone: None,
            compare: None,
        };
        let _ = values(
            auth_user.clone(),
            Path(site_id),
            Query(qr_val.clone()),
            State(state.clone()),
        )
        .await;

        let pa_query = PageAnalyticsQuery {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            limit: Some(10),
            url_path: None,
        };
        let _ = entry_exit(
            auth_user.clone(),
            Path(site_id),
            Query(pa_query.clone()),
            State(state.clone()),
        )
        .await;

        let pa_query_2 = PageAnalyticsQuery {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            limit: Some(10),
            url_path: None,
        };
        let _ = entry_pages(
            auth_user.clone(),
            Path(site_id),
            Query(pa_query_2.clone()),
            State(state.clone()),
        )
        .await;

        let pa_query_3 = PageAnalyticsQuery {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            limit: Some(10),
            url_path: None,
        };
        let _ = exit_pages(
            auth_user.clone(),
            Path(site_id),
            Query(pa_query_3.clone()),
            State(state.clone()),
        )
        .await;

        let pa_query_4 = PageAnalyticsQuery {
            start_at: Some((now - Duration::days(7)).timestamp_millis()),
            end_at: Some(now.timestamp_millis()),
            limit: Some(10),
            url_path: None,
        };
        let _ = engagement(
            auth_user.clone(),
            Path(site_id),
            Query(pa_query_4.clone()),
            State(state.clone()),
        )
        .await;

        assert!(!check_website_access(&state.pool, stranger_id, false, site_id).await);
        assert!(!check_website_write_access(&state.pool, stranger_id, false, site_id).await);
        assert!(check_website_access(&state.pool, stranger_id, true, site_id).await);
        assert!(check_website_write_access(&state.pool, stranger_id, true, site_id).await);

        assert!(
            update(
                stranger_auth.clone(),
                Path(site_id),
                State(state.clone()),
                Json(json!({}))
            )
            .await
            .is_err()
        );
        assert!(
            delete(stranger_auth.clone(), Path(site_id), State(state.clone()))
                .await
                .is_err()
        );
        assert!(
            stats(
                stranger_auth.clone(),
                Path(site_id),
                Query(qr.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            active(stranger_auth.clone(), Path(site_id), State(state.clone()))
                .await
                .is_err()
        );
        assert!(
            daterange(stranger_auth.clone(), Path(site_id), State(state.clone()))
                .await
                .is_err()
        );
        assert!(
            metrics(
                stranger_auth.clone(),
                Path(site_id),
                Query(qr_m.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            metrics_expanded(
                stranger_auth.clone(),
                Path(site_id),
                Query(qr_m.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            pageviews(
                stranger_auth.clone(),
                Path(site_id),
                Query(qr_pv.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            values(
                stranger_auth.clone(),
                Path(site_id),
                Query(qr_val.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            reset(stranger_auth.clone(), Path(site_id), State(state.clone()))
                .await
                .is_err()
        );
        assert!(
            transfer(
                stranger_auth.clone(),
                Path(site_id),
                State(state.clone()),
                Json(json!({}))
            )
            .await
            .is_err()
        );
        assert!(
            entry_exit(
                stranger_auth.clone(),
                Path(site_id),
                Query(pa_query.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            entry_pages(
                stranger_auth.clone(),
                Path(site_id),
                Query(pa_query_2.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            exit_pages(
                stranger_auth.clone(),
                Path(site_id),
                Query(pa_query_3.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            engagement(
                stranger_auth.clone(),
                Path(site_id),
                Query(pa_query_4.clone()),
                State(state.clone())
            )
            .await
            .is_err()
        );

        let res_reset = reset(auth_user.clone(), Path(site_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_reset.0["ok"], true);

        let res_transfer_user = transfer(
            auth_user.clone(),
            Path(site_id),
            State(state.clone()),
            Json(json!({ "userId": stranger_id.to_string() })),
        )
        .await;
        assert!(res_transfer_user.is_err());
        assert_eq!(res_transfer_user.unwrap_err().0, StatusCode::FORBIDDEN);

        let res_transfer_admin = transfer(
            admin_auth.clone(),
            Path(site_id),
            State(state.clone()),
            Json(json!({ "userId": stranger_id.to_string() })),
        )
        .await
        .unwrap();
        assert_eq!(res_transfer_admin.0["userId"], stranger_id.to_string());

        let test_team_id = Uuid::now_v7();
        let _ = sqlx::query(r#"INSERT INTO "team" (team_id, name, access_code) VALUES ($1, 'Web Team', 'code_web')"#)
            .bind(test_team_id)
            .execute(&pool)
            .await;
        let res_transfer_team = transfer(
            admin_auth.clone(),
            Path(site_id),
            State(state.clone()),
            Json(json!({ "teamId": test_team_id.to_string() })),
        )
        .await
        .unwrap();
        assert_eq!(res_transfer_team.0["teamId"], test_team_id.to_string());
        let _ = sqlx::query(r#"DELETE FROM "team" WHERE team_id = $1"#)
            .bind(test_team_id)
            .execute(&pool)
            .await;

        let res_transfer_empty = transfer(
            admin_auth.clone(),
            Path(site_id),
            State(state.clone()),
            Json(json!({})),
        )
        .await;
        assert!(res_transfer_empty.is_ok());

        let res_del = delete(admin_auth.clone(), Path(site_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_del.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(site_id)
            .execute(&pool)
            .await;
        for u in [&auth_user, &stranger_auth, &admin_auth] {
            let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
                .bind(u.user_id)
                .execute(&pool)
                .await;
        }

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

        assert!(
            get(admin_auth.clone(), Path(site_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            list(auth_user.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            create(
                auth_user.clone(),
                State(err_state.clone()),
                Json(json!({"name":"Fail","domain":"fail.com"}))
            )
            .await
            .is_err()
        );
        assert!(
            charts(Query(charts_query), State(err_state.clone()))
                .await
                .is_ok()
        );
        assert!(
            update(
                admin_auth.clone(),
                Path(site_id),
                State(err_state.clone()),
                Json(json!({"name":"Fail"}))
            )
            .await
            .is_err()
        );
        assert!(
            delete(admin_auth.clone(), Path(site_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            stats(
                admin_auth.clone(),
                Path(site_id),
                Query(qr),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            active(admin_auth.clone(), Path(site_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            daterange(admin_auth.clone(), Path(site_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            metrics(
                admin_auth.clone(),
                Path(site_id),
                Query(qr_m.clone()),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            metrics_expanded(
                admin_auth.clone(),
                Path(site_id),
                Query(qr_m),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            pageviews(
                admin_auth.clone(),
                Path(site_id),
                Query(qr_pv),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            values(
                admin_auth.clone(),
                Path(site_id),
                Query(qr_val),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(
            reset(admin_auth.clone(), Path(site_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            transfer(
                admin_auth.clone(),
                Path(site_id),
                State(err_state.clone()),
                Json(json!({"userId": stranger_id.to_string()}))
            )
            .await
            .is_err()
        );
        assert!(
            transfer(
                admin_auth.clone(),
                Path(site_id),
                State(err_state.clone()),
                Json(json!({"teamId": stranger_id.to_string()}))
            )
            .await
            .is_err()
        );
        assert!(
            entry_exit(
                admin_auth.clone(),
                Path(site_id),
                Query(pa_query),
                State(err_state.clone())
            )
            .await
            .is_ok()
        );
        assert!(
            engagement(
                admin_auth.clone(),
                Path(site_id),
                Query(pa_query_4),
                State(err_state.clone())
            )
            .await
            .is_ok()
        );
    }
}
