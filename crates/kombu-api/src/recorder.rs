#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

pub async fn record(
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
    let session_id = body["sessionId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "sessionId required" })),
            )
        })?;
    let visit_id = body["visitId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::now_v7);

    let chunk_index = body["chunkIndex"].as_i64().unwrap_or(0) as i32;
    let events = body["events"].to_string().into_bytes();
    let event_count = body["eventCount"].as_i64().unwrap_or(0) as i32;
    let replay_id = Uuid::now_v7();

    sqlx::query(
        r#"
        INSERT INTO "session_replay" (
            replay_id, website_id, session_id, visit_id, chunk_index, events, event_count,
            started_at, ended_at, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW(), NOW())
        "#,
    )
    .bind(replay_id)
    .bind(website_id)
    .bind(session_id)
    .bind(visit_id)
    .bind(chunk_index)
    .bind(events)
    .bind(event_count)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({ "ok": true, "replayId": replay_id })))
}

pub async fn recorder_config(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as::<_, (bool, Option<serde_json::Value>)>(
        r#"SELECT recorder_enabled, replay_config FROM "website" WHERE website_id = $1"#,
    )
    .bind(website_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some((enabled, config)) => Ok(Json(json!({
            "recorderEnabled": enabled,
            "replayConfig": config.unwrap_or(json!({}))
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Website not found" })),
        )),
    }
}

pub async fn list_replays(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                replay_id as id,
                session_id as "sessionId",
                visit_id as "visitId",
                event_count as "eventCount",
                started_at as "startedAt",
                ended_at as "endedAt",
                created_at as "createdAt"
            FROM "session_replay"
            WHERE website_id = $1
            ORDER BY created_at DESC
            LIMIT 50
        ) t
        "#,
    )
    .bind(website_id)
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

pub async fn list_session_replays(
    Path((website_id, session_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                replay_id as id,
                session_id as "sessionId",
                visit_id as "visitId",
                event_count as "eventCount",
                started_at as "startedAt",
                ended_at as "endedAt",
                created_at as "createdAt"
            FROM "session_replay"
            WHERE website_id = $1 AND session_id = $2
            ORDER BY created_at DESC
            LIMIT 50
        ) t
        "#,
    )
    .bind(website_id)
    .bind(session_id)
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

pub async fn list_saved_replays(
    Path(website_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    list_replays(Path(website_id), State(state)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveReplayPayload {
    pub is_saved: Option<bool>,
    pub name: Option<String>,
}

pub async fn get_saved_replay(
    Path((website_id, replay_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM session_replay_saved WHERE website_id = $1 AND (saved_replay_id = $2 OR visit_id = $2))",
    )
    .bind(website_id)
    .bind(replay_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    Ok(Json(json!({ "isSaved": exists })))
}

pub async fn save_replay(
    Path((website_id, replay_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(payload): Json<SaveReplayPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let is_saved = payload.is_saved.unwrap_or(true);

    if is_saved {
        let name = payload.name.unwrap_or_else(|| "Saved Replay".to_string());
        sqlx::query(
            "INSERT INTO session_replay_saved (saved_replay_id, website_id, visit_id, name) VALUES ($1, $2, $1, $3) ON CONFLICT (website_id, visit_id) DO UPDATE SET name = EXCLUDED.name",
        )
        .bind(replay_id)
        .bind(website_id)
        .bind(&name)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    } else {
        sqlx::query(
            "DELETE FROM session_replay_saved WHERE website_id = $1 AND (saved_replay_id = $2 OR visit_id = $2)",
        )
        .bind(website_id)
        .bind(replay_id)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    }

    Ok(Json(json!({ "ok": true, "isSaved": is_saved })))
}

pub async fn delete_saved_replay(
    Path((website_id, replay_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(
        "DELETE FROM session_replay_saved WHERE website_id = $1 AND (saved_replay_id = $2 OR visit_id = $2)",
    )
    .bind(website_id)
    .bind(replay_id)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn get_replay(
    Path((website_id, replay_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as::<_, (Uuid, Uuid, i32, Vec<u8>)>(
        "SELECT replay_id, session_id, event_count, events FROM \"session_replay\" WHERE website_id = $1 AND replay_id = $2",
    )
    .bind(website_id)
    .bind(replay_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    match row {
        Some((id, session_id, event_count, events_bytes)) => {
            let events_str = String::from_utf8_lossy(&events_bytes);
            let events_val: Value = serde_json::from_str(&events_str).unwrap_or(json!([]));
            Ok(Json(json!({
                "id": id,
                "sessionId": session_id,
                "eventCount": event_count,
                "events": events_val
            })))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Replay not found" })),
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_recorder_endpoints_full() {
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

        assert!(record(State(state.clone()), Json(json!({}))).await.is_err());
        assert!(
            record(
                State(state.clone()),
                Json(json!({ "websiteId": website_id }))
            )
            .await
            .is_err()
        );

        let _ = sqlx::query(
            r#"INSERT INTO "website" (website_id, name, domain, recorder_enabled, replay_config) VALUES ($1, $2, $3, true, '{"sampleRate": 1.0}')"#,
        )
        .bind(website_id)
        .bind("Recorder Web")
        .bind(format!("rec-{website_id}.com"))
        .execute(&pool)
        .await;

        let _ = sqlx::query(
            r#"INSERT INTO "session" (session_id, website_id, hostname, browser, os, device, screen, language, country, distinct_id)
               VALUES ($1, $2, 'localhost', 'Chrome', 'Linux', 'desktop', '1920x1080', 'en', 'US', 'dist1')"#,
        )
        .bind(session_id)
        .bind(website_id)
        .execute(&pool)
        .await;

        let res_rec = record(
            State(state.clone()),
            Json(json!({
                "websiteId": website_id,
                "sessionId": session_id,
                "chunkIndex": 1,
                "events": [{"type": 1}],
                "eventCount": 1
            })),
        )
        .await;
        assert!(res_rec.is_ok());
        let replay_id: Uuid = res_rec.unwrap().0["replayId"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        let res_cfg = recorder_config(Path(website_id), State(state.clone())).await;
        assert!(res_cfg.is_ok());
        let res_cfg_404 = recorder_config(Path(Uuid::now_v7()), State(state.clone())).await;
        assert!(res_cfg_404.is_err());

        assert!(
            list_replays(Path(website_id), State(state.clone()))
                .await
                .is_ok()
        );
        assert!(
            list_session_replays(Path((website_id, session_id)), State(state.clone()))
                .await
                .is_ok()
        );
        assert!(
            list_saved_replays(Path(website_id), State(state.clone()))
                .await
                .is_ok()
        );

        let gsr_res = get_saved_replay(Path((website_id, replay_id)), State(state.clone())).await;
        assert!(gsr_res.is_ok());
        let res_save = save_replay(
            Path((website_id, replay_id)),
            State(state.clone()),
            Json(SaveReplayPayload {
                is_saved: Some(true),
                name: Some("Custom Replay".into()),
            }),
        )
        .await;
        assert!(res_save.is_ok());

        let res_unsave = save_replay(
            Path((website_id, replay_id)),
            State(state.clone()),
            Json(SaveReplayPayload {
                is_saved: Some(false),
                name: None,
            }),
        )
        .await;
        assert!(res_unsave.is_ok());

        let res_del =
            delete_saved_replay(Path((website_id, replay_id)), State(state.clone())).await;
        assert!(res_del.is_ok());

        let res_get_rep = get_replay(Path((website_id, replay_id)), State(state.clone())).await;
        assert!(res_get_rep.is_ok());
        let res_get_rep_404 =
            get_replay(Path((website_id, Uuid::now_v7())), State(state.clone())).await;
        assert!(res_get_rep_404.is_err());

        let _ = sqlx::query(r#"DELETE FROM "session_replay" WHERE replay_id = $1"#)
            .bind(replay_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "session" WHERE session_id = $1"#)
            .bind(session_id)
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

        assert!(
            record(
                State(err_state.clone()),
                Json(json!({"websiteId": website_id, "sessionId": session_id}))
            )
            .await
            .is_err()
        );
        assert!(
            recorder_config(Path(website_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            list_replays(Path(website_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            list_session_replays(Path((website_id, session_id)), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            get_saved_replay(Path((website_id, replay_id)), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            save_replay(
                Path((website_id, replay_id)),
                State(err_state.clone()),
                Json(SaveReplayPayload {
                    is_saved: Some(true),
                    name: None
                })
            )
            .await
            .is_err()
        );
        assert!(
            save_replay(
                Path((website_id, replay_id)),
                State(err_state.clone()),
                Json(SaveReplayPayload {
                    is_saved: Some(false),
                    name: None
                })
            )
            .await
            .is_err()
        );
        assert!(
            delete_saved_replay(Path((website_id, replay_id)), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            get_replay(Path((website_id, replay_id)), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
