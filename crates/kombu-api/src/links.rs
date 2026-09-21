#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

fn is_social_crawler(user_agent: &str) -> bool {
    let ua = user_agent.to_ascii_lowercase();
    ua.contains("twitterbot")
        || ua.contains("facebookexternalhit")
        || ua.contains("linkedinbot")
        || ua.contains("slackbot")
        || ua.contains("telegrambot")
        || ua.contains("discordbot")
        || ua.contains("whatsapp")
        || ua.contains("pinterest")
        || ua.contains("vkshare")
        || ua.contains("googlebot")
        || ua.contains("bingbot")
}

pub async fn redirect_slug(
    Path(slug): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Response, (StatusCode, &'static str)> {
    let row = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT link_id, name, url, og_title, og_description, og_image_url
        FROM "link"
        WHERE slug = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    let Some((link_id, name, destination_url, og_title, og_description, og_image_url)) = row else {
        return Err((StatusCode::NOT_FOUND, "Link not found"));
    };

    let pool = state.pool.clone();
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let custom_ip_header = std::env::var("CLIENT_IP_HEADER").ok();
    let header_iter = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.as_str(), s)));
    let ip = kombu_core::ip::resolve_client_ip(header_iter, custom_ip_header.as_deref())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let referrer = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let dest_url_clone = destination_url.clone();
    let ua_clone = user_agent.clone();

    tokio::spawn(async move {
        let created_at = Utc::now();
        let session_id =
            kombu_ingest::generate_session_id(link_id, &ip, &ua_clone, "secret", created_at);
        let visit_id = kombu_ingest::generate_visit_id(session_id, created_at);
        let payload = kombu_ingest::CollectData {
            website: None,
            link: Some(link_id.to_string()),
            pixel: None,
            hostname: None,
            language: None,
            referrer,
            screen: None,
            title: None,
            url: Some(dest_url_clone),
            name: None,
            data: None,
            tag: None,
            ip: Some(ip),
            user_agent: Some(ua_clone),
            timestamp: None,
            id: None,
            browser: None,
            os: None,
            device: None,
            lcp: None,
            inp: None,
            cls: None,
            fcp: None,
            ttfb: None,
            event_type: Some(kombu_core::constants::EVENT_TYPE_LINK_EVENT),
            ..Default::default()
        };
        let _ = kombu_ingest::save_session_and_event(
            &pool, link_id, session_id, visit_id, &payload, created_at, "desktop", None, None,
            None, None, None,
        )
        .await;
    });

    if is_social_crawler(&user_agent) {
        let title = og_title.as_deref().unwrap_or(&name);
        let desc = og_description.as_deref().unwrap_or("");
        let image_meta = og_image_url
            .as_ref()
            .map(|img| format!(r#"<meta property="og:image" content="{img}">"#))
            .unwrap_or_default();

        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>{title}</title>
  <meta property="og:title" content="{title}">
  <meta property="og:description" content="{desc}">
  <meta property="og:url" content="{destination_url}">
  {image_meta}
  <meta http-equiv="refresh" content="0;url={destination_url}">
</head>
<body>
  <p>Redirecting to <a href="{destination_url}">{destination_url}</a>...</p>
</body>
</html>"#
        );
        return Ok((
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            Html(html),
        )
            .into_response());
    }

    Ok(Redirect::temporary(&destination_url).into_response())
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                link_id as id,
                name,
                url,
                slug,
                og_title as "ogTitle",
                og_description as "ogDescription",
                og_image_url as "ogImageUrl",
                user_id as "userId",
                team_id as "teamId",
                created_at as "createdAt"
            FROM "link"
            WHERE deleted_at IS NULL
            ORDER BY created_at DESC
        ) t
        "#,
    )
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

pub async fn create(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let link_id = Uuid::now_v7();
    let name = body["name"].as_str().unwrap_or("Link");
    let url = body["url"].as_str().unwrap_or("https://example.com");
    let slug = body["slug"].as_str().unwrap_or("link");
    let og_title = body["ogTitle"]
        .as_str()
        .or_else(|| body["og_title"].as_str());
    let og_description = body["ogDescription"]
        .as_str()
        .or_else(|| body["og_description"].as_str());
    let og_image_url = body["ogImageUrl"]
        .as_str()
        .or_else(|| body["og_image_url"].as_str());
    let team_id = body["teamId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());

    let auth_user_id = crate::auth::get_user_id_from_headers(&headers);
    let default_admin_id = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT user_id FROM "user" WHERE role = 'admin' AND deleted_at IS NULL ORDER BY created_at ASC LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let effective_user_id = auth_user_id.or(default_admin_id);
    let assigned_user_id = if team_id.is_some() {
        None
    } else {
        effective_user_id
    };

    sqlx::query(
        r#"
        INSERT INTO "link" (link_id, name, url, slug, og_title, og_description, og_image_url, user_id, team_id, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
        "#,
    )
    .bind(link_id)
    .bind(name)
    .bind(url)
    .bind(slug)
    .bind(og_title)
    .bind(og_description)
    .bind(og_image_url)
    .bind(assigned_user_id)
    .bind(team_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(link_id), State(state)).await
}

pub async fn get(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                link_id as id,
                name,
                url,
                slug,
                og_title as "ogTitle",
                og_description as "ogDescription",
                og_image_url as "ogImageUrl",
                user_id as "userId",
                team_id as "teamId",
                created_at as "createdAt"
            FROM "link"
            WHERE link_id = $1 AND deleted_at IS NULL
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
        Some(l) => Ok(Json(l)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Link not found" })),
        )),
    }
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str();
    let url = body["url"].as_str();
    let og_title = body["ogTitle"]
        .as_str()
        .or_else(|| body["og_title"].as_str());
    let og_description = body["ogDescription"]
        .as_str()
        .or_else(|| body["og_description"].as_str());
    let og_image_url = body["ogImageUrl"]
        .as_str()
        .or_else(|| body["og_image_url"].as_str());

    sqlx::query(
        r#"
        UPDATE "link"
        SET
            name = COALESCE($2, name),
            url = COALESCE($3, url),
            og_title = COALESCE($4, og_title),
            og_description = COALESCE($5, og_description),
            og_image_url = COALESCE($6, og_image_url),
            updated_at = NOW()
        WHERE link_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(url)
    .bind(og_title)
    .bind(og_description)
    .bind(og_image_url)
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
    sqlx::query(r#"UPDATE "link" SET deleted_at = NOW() WHERE link_id = $1"#)
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_links_endpoints_full() {
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

        let slug = format!("slug_{}", Uuid::now_v7());

        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "Mozilla/5.0".parse().unwrap());
        let body_create = json!({
            "name": "Full Test Link",
            "url": "https://example.com/dest",
            "slug": slug,
            "ogTitle": "OpenGraph Title",
            "ogDescription": "OpenGraph Desc",
            "ogImageUrl": "https://example.com/og.png"
        });
        let res_create = create(headers.clone(), State(state.clone()), Json(body_create)).await;
        assert!(res_create.is_ok());
        let link_id: Uuid = res_create.unwrap().0["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        let team_id = Uuid::now_v7();
        let _ = sqlx::query(r#"INSERT INTO "team" (team_id, name, access_code) VALUES ($1, 'Team Link', 'code123')"#)
            .bind(team_id)
            .execute(&pool)
            .await;
        let slug_team = format!("slug_team_{}", Uuid::now_v7());
        let res_create_team = create(
            headers.clone(),
            State(state.clone()),
            Json(json!({
                "name": "Team Link",
                "url": "https://example.com",
                "slug": slug_team,
                "teamId": team_id
            })),
        )
        .await;
        assert!(res_create_team.is_ok());
        let team_link_id: Uuid = res_create_team.unwrap().0["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let _ = delete(Path(team_link_id), State(state.clone())).await;
        let _ = sqlx::query(r#"DELETE FROM "team" WHERE team_id = $1"#)
            .bind(team_id)
            .execute(&pool)
            .await;

        assert!(list(State(state.clone())).await.is_ok());

        assert!(get(Path(link_id), State(state.clone())).await.is_ok());
        assert!(
            get(Path(Uuid::now_v7()), State(state.clone()))
                .await
                .is_err()
        );

        let body_upd = json!({
            "name": "Updated Link Name",
            "og_title": "Upd OG"
        });
        assert!(
            update(Path(link_id), State(state.clone()), Json(body_upd))
                .await
                .is_ok()
        );

        let mut redir_headers = headers.clone();
        redir_headers.insert("referer", "https://referrer.example.com".parse().unwrap());
        let res_redir =
            redirect_slug(Path(slug.clone()), redir_headers, State(state.clone())).await;
        assert!(res_redir.is_ok());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut bot_headers = HeaderMap::new();
        bot_headers.insert("user-agent", "Twitterbot/1.0".parse().unwrap());
        let res_bot_redir =
            redirect_slug(Path(slug.clone()), bot_headers, State(state.clone())).await;
        assert!(res_bot_redir.is_ok());

        let res_404_redir = redirect_slug(
            Path("nonexistent_slug_xyz".into()),
            headers.clone(),
            State(state.clone()),
        )
        .await;
        assert!(res_404_redir.is_err());

        assert!(delete(Path(link_id), State(state.clone())).await.is_ok());

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
            redirect_slug(
                Path(slug.clone()),
                headers.clone(),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
        assert!(list(State(err_state.clone())).await.is_err());
        assert!(
            create(headers.clone(), State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
        assert!(get(Path(link_id), State(err_state.clone())).await.is_err());
        assert!(
            update(Path(link_id), State(err_state.clone()), Json(json!({})))
                .await
                .is_err()
        );
        assert!(
            delete(Path(link_id), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
