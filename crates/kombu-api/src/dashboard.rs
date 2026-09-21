#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::get_user_id_from_headers;
use crate::router::AppState;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OverviewQuery {
    #[serde(alias = "start_at")]
    pub start_at: Option<i64>,
    #[serde(alias = "end_at")]
    pub end_at: Option<i64>,
    pub unit: Option<String>,
    pub limit: Option<i64>,
}

pub async fn overview(
    headers: HeaderMap,
    Query(query): Query<OverviewQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = get_user_id_from_headers(&headers);
    let end_at = query
        .end_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(Utc::now);
    let start_at = query
        .start_at
        .and_then(DateTime::from_timestamp_millis)
        .unwrap_or_else(|| end_at - Duration::days(30));
    let unit = query.unit.as_deref().unwrap_or("day");

    let is_admin = if let Some(uid) = user_id {
        sqlx::query_scalar::<_, String>(
            r#"SELECT role FROM "user" WHERE user_id = $1 AND deleted_at IS NULL"#,
        )
        .bind(uid)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or_default()
        .is_some_and(|r| r == "admin")
    } else {
        false
    };

    let websites: Vec<(Uuid, String, Option<String>)> = if is_admin || user_id.is_none() {
        sqlx::query_as::<_, (Uuid, String, Option<String>)>(
            r#"
            SELECT website_id, name, domain
            FROM "website"
            WHERE deleted_at IS NULL
            ORDER BY name ASC
            LIMIT $1
            "#,
        )
        .bind(query.limit.unwrap_or(50))
        .fetch_all(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?
    } else {
        let uid = user_id.unwrap_or_default();
        sqlx::query_as::<_, (Uuid, String, Option<String>)>(
            r#"
            SELECT w.website_id, w.name, w.domain
            FROM "website" w
            LEFT JOIN "team_user" tu ON tu.team_id = w.team_id AND tu.user_id = $1
            WHERE w.deleted_at IS NULL AND (w.user_id = $1 OR tu.user_id IS NOT NULL)
            ORDER BY w.name ASC
            LIMIT $2
            "#,
        )
        .bind(uid)
        .bind(query.limit.unwrap_or(50))
        .fetch_all(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?
    };

    let mut total_pageviews = 0i64;
    let mut total_visitors = 0i64;
    let mut total_visits = 0i64;
    let mut total_bounces = 0i64;
    let mut total_time = 0i64;

    let mut website_items = Vec::new();

    for (w_id, name, domain) in websites {
        let stats = crate::storage::website_stats(&state.pool, w_id, start_at, end_at)
            .await
            .unwrap_or(kombu_query::WebsiteStats {
                pageviews: 0,
                visitors: 0,
                visits: 0,
                bounces: 0,
                totaltime: 0,
            });

        let series = kombu_query::get_pageview_stats(&state.pool, w_id, start_at, end_at, unit)
            .await
            .unwrap_or_default();

        total_pageviews += stats.pageviews;
        total_visitors += stats.visitors;
        total_visits += stats.visits;
        total_bounces += stats.bounces;
        total_time += stats.totaltime;

        website_items.push(json!({
            "id": w_id,
            "name": name,
            "domain": domain,
            "pageviews": stats.pageviews,
            "visitors": stats.visitors,
            "visits": stats.visits,
            "bounces": stats.bounces,
            "totaltime": stats.totaltime,
            "series": series
        }));
    }

    Ok(Json(json!({
        "total": {
            "pageviews": total_pageviews,
            "visitors": total_visitors,
            "visits": total_visits,
            "bounces": total_bounces,
            "totaltime": total_time
        },
        "websites": website_items
    })))
}

pub async fn get(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = get_user_id_from_headers(&headers).unwrap_or_else(Uuid::nil);

    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                board_id as id,
                user_id as "userId",
                type,
                name,
                description,
                parameters,
                created_at as "createdAt"
            FROM "board"
            WHERE user_id = $1 AND type = 'dashboard'
            LIMIT 1
        ) t
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if let Some(board) = row {
        Ok(Json(board))
    } else {
        Ok(Json(json!({
            "id": user_id,
            "type": "dashboard",
            "name": "Dashboard",
            "description": "",
            "parameters": {
                "rows": [
                    {
                        "id": Uuid::now_v7().to_string(),
                        "columns": [
                            {
                                "id": Uuid::now_v7().to_string(),
                                "component": null
                            }
                        ]
                    }
                ]
            }
        })))
    }
}

pub async fn save(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = get_user_id_from_headers(&headers).unwrap_or_else(Uuid::nil);
    let name = body["name"].as_str().unwrap_or("Dashboard");
    let description = body["description"].as_str().unwrap_or("");
    let parameters = body.get("parameters").cloned().unwrap_or_else(|| json!({}));

    let existing_id = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT board_id FROM "board" WHERE user_id = $1 AND type = 'dashboard' LIMIT 1"#,
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let board_id = existing_id.unwrap_or(user_id);

    sqlx::query(
        r#"
        INSERT INTO "board" (board_id, user_id, type, name, description, parameters, created_at, updated_at)
        VALUES ($1, $2, 'dashboard', $3, $4, $5, NOW(), NOW())
        ON CONFLICT (board_id) DO UPDATE SET
            name = EXCLUDED.name,
            description = EXCLUDED.description,
            parameters = EXCLUDED.parameters,
            updated_at = NOW()
        "#,
    )
    .bind(board_id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(parameters.clone())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "id": board_id,
        "userId": user_id,
        "type": "dashboard",
        "name": name,
        "description": description,
        "parameters": parameters
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[tokio::test]
    async fn test_dashboard_get_and_save() {
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
        let username = format!("dash_{}", user_id.simple());
        let pass_hash = crate::auth::hash_password("ValidPassword123!").unwrap();

        sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role, created_at, updated_at) VALUES ($1, $2, $3, 'user', NOW(), NOW())"#,
        )
        .bind(user_id)
        .bind(&username)
        .bind(&pass_hash)
        .execute(&pool)
        .await
        .unwrap();

        let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
        let claims = crate::auth::Claims {
            user_id,
            username,
            role: "user".into(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );

        let res_default = get(headers.clone(), State(state.clone())).await.unwrap();
        assert_eq!(res_default.0["type"], "dashboard");
        assert_eq!(res_default.0["name"], "Dashboard");

        let body = json!({
            "name": "Custom Dashboard",
            "description": "My test dashboard",
            "parameters": { "metrics": ["pageviews", "visitors"] }
        });
        let res_save = save(headers.clone(), State(state.clone()), Json(body))
            .await
            .unwrap();
        assert_eq!(res_save.0["name"], "Custom Dashboard");
        assert_eq!(res_save.0["description"], "My test dashboard");

        let res_saved = get(headers.clone(), State(state.clone())).await.unwrap();
        assert_eq!(res_saved.0["name"], "Custom Dashboard");

        let body_update = json!({
            "name": "Updated Dashboard",
            "description": "Updated description",
            "parameters": { "metrics": ["revenue"] }
        });
        let res_update = save(headers.clone(), State(state.clone()), Json(body_update))
            .await
            .unwrap();
        assert_eq!(res_update.0["name"], "Updated Dashboard");

        let _ = sqlx::query(r#"DELETE FROM "board" WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await;

        let q = Query(OverviewQuery {
            start_at: None,
            end_at: None,
            unit: None,
            limit: Some(10),
        });
        assert!(
            overview(HeaderMap::new(), q.clone(), State(state.clone()))
                .await
                .is_ok()
        );
        assert!(
            overview(headers.clone(), q.clone(), State(state.clone()))
                .await
                .is_ok()
        );

        let admin_user_id = Uuid::now_v7();
        let _ = sqlx::query(
            r#"INSERT INTO "user" (user_id, username, password, role, created_at, updated_at) VALUES ($1, $2, 'pass', 'admin', NOW(), NOW())"#,
        )
        .bind(admin_user_id)
        .bind(format!("admin_{}", admin_user_id.simple()))
        .execute(&pool)
        .await;

        let admin_claims = crate::auth::Claims {
            user_id: admin_user_id,
            username: "admin_user".into(),
            role: "admin".into(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
        };
        let admin_token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &admin_claims,
            &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();
        let mut admin_headers = HeaderMap::new();
        admin_headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {admin_token}")).unwrap(),
        );
        assert!(
            overview(admin_headers, q.clone(), State(state.clone()))
                .await
                .is_ok()
        );
        let _ = sqlx::query(r#"DELETE FROM "user" WHERE user_id = $1"#)
            .bind(admin_user_id)
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

        assert!(
            overview(HeaderMap::new(), q.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            overview(headers.clone(), q.clone(), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            save(headers.clone(), State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
    }
}
