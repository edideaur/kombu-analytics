#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

pub fn generate_slug(len: usize) -> String {
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let u = Uuid::new_v4();
        for &b in u.as_bytes() {
            if out.len() >= len {
                break;
            }
            out.push(CHARS[(b as usize) % CHARS.len()] as char);
        }
    }
    out
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                share_id as id,
                share_id as "shareId",
                entity_id as "entityId",
                name,
                slug,
                share_type as "shareType",
                parameters,
                created_at as "createdAt",
                updated_at as "updatedAt"
            FROM "share"
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

    Ok(Json(rows))
}

pub async fn get_entity_shares(
    Path(entity_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                share_id as id,
                share_id as "shareId",
                entity_id as "entityId",
                name,
                slug,
                share_type as "shareType",
                parameters,
                created_at as "createdAt",
                updated_at as "updatedAt"
            FROM "share"
            WHERE entity_id = $1
            ORDER BY created_at DESC
        ) t
        "#,
    )
    .bind(entity_id)
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

pub async fn create_board_share(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    create_entity_share(&state, id, 4, body).await
}

pub async fn create_website_share(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    create_entity_share(&state, id, 1, body).await
}

pub async fn create_link_share(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    create_entity_share(&state, id, 2, body).await
}

pub async fn create_pixel_share(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    create_entity_share(&state, id, 3, body).await
}

async fn create_entity_share(
    state: &AppState,
    entity_id: Uuid,
    share_type: i32,
    body: Value,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str().unwrap_or("Share");
    let slug = body["slug"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| generate_slug(16), ToString::to_string);

    if slug.contains('/') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Slug cannot contain slashes" })),
        ));
    }
    if slug.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Slug cannot exceed 100 characters" })),
        ));
    }

    let existing = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM "share"
        WHERE slug = $1
           OR parameters->'aliases' @> to_jsonb($1::text)
        "#,
    )
    .bind(&slug)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if existing > 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "That slug is already taken." })),
        ));
    }

    let share_id = Uuid::now_v7();
    let parameters = body.get("parameters").cloned().unwrap_or_else(|| json!({}));

    sqlx::query(
        r#"
        INSERT INTO "share" (share_id, entity_id, name, share_type, slug, parameters, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
        "#,
    )
    .bind(share_id)
    .bind(entity_id)
    .bind(name)
    .bind(share_type)
    .bind(&slug)
    .bind(&parameters)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(json!({
        "id": share_id,
        "shareId": share_id,
        "entityId": entity_id,
        "name": name,
        "shareType": share_type,
        "slug": slug,
        "parameters": parameters,
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339()
    })))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let entity_id = body["entityId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "entityId required" })),
            )
        })?;
    let share_type = body["shareType"].as_i64().unwrap_or(1) as i32;
    create_entity_share(&state, entity_id, share_type, body).await
}

pub async fn get_by_id(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                share_id as id,
                share_id as "shareId",
                entity_id as "entityId",
                name,
                slug,
                share_type as "shareType",
                parameters,
                created_at as "createdAt",
                updated_at as "updatedAt"
            FROM "share"
            WHERE share_id = $1
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
        Some(s) => Ok(Json(s)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Share not found" })),
        )),
    }
}

pub async fn update_by_id(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str().map(str::trim);
    let new_slug = body["slug"].as_str().map(str::trim);
    let mut parameters = body.get("parameters").cloned();

    if let Some(s) = new_slug {
        if s.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Slug cannot be empty" })),
            ));
        }
        if s.contains('/') {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Slug cannot contain slashes" })),
            ));
        }
        if s.len() > 100 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Slug cannot exceed 100 characters" })),
            ));
        }
    }

    let current_share = sqlx::query_as::<_, (String, Value)>(
        r#"SELECT slug, parameters FROM "share" WHERE share_id = $1"#,
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

    let Some((current_slug, current_parameters)) = current_share else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Share not found" })),
        ));
    };

    if let Some(ref mut p) = parameters {
        if let (None, Some(existing_aliases)) =
            (p.get("aliases"), current_parameters.get("aliases"))
        {
            p["aliases"] = existing_aliases.clone();
        }
    }

    if let Some(s) = new_slug {
        let existing = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM "share"
            WHERE share_id != $1
              AND (
                  slug = $2
                  OR parameters->'aliases' @> to_jsonb($2::text)
              )
            "#,
        )
        .bind(id)
        .bind(s)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

        if existing > 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "That slug is already taken." })),
            ));
        }

        if s != current_slug {
            let mut params_obj = parameters.unwrap_or_else(|| current_parameters.clone());
            if !params_obj.is_object() {
                params_obj = json!({});
            }
            let mut aliases: Vec<String> = params_obj
                .get("aliases")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            if !aliases.iter().any(|a| a == &current_slug) {
                aliases.push(current_slug.clone());
            }
            aliases.retain(|a| a != s);
            params_obj["aliases"] = json!(aliases);
            parameters = Some(params_obj);
        }
    }

    sqlx::query(
        r#"
        UPDATE "share"
        SET
            name = COALESCE($2, name),
            slug = COALESCE($3, slug),
            parameters = COALESCE($4, parameters),
            updated_at = NOW()
        WHERE share_id = $1
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(new_slug)
    .bind(parameters)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get_by_id(Path(id), State(state)).await
}

pub async fn delete_by_id(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(r#"DELETE FROM "share" WHERE share_id = $1"#)
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

pub async fn get_by_slug(
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            i32,
            Value,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT
            share_id,
            entity_id,
            name,
            slug,
            share_type,
            parameters,
            created_at
        FROM "share"
        WHERE slug = $1
           OR parameters->'aliases' @> to_jsonb($1::text)
        ORDER BY (slug = $1) DESC
        LIMIT 1
        "#,
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let Some((share_id, entity_id, name, slug_val, share_type, parameters, created_at)) = row
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Share not found" })),
        ));
    };

    let mut data = serde_json::Map::new();
    data.insert("id".into(), json!(share_id));
    data.insert("shareId".into(), json!(share_id));
    data.insert("entityId".into(), json!(entity_id));
    data.insert("name".into(), json!(name));
    data.insert("slug".into(), json!(slug_val));
    data.insert("queriedSlug".into(), json!(slug));
    data.insert("shareType".into(), json!(share_type));
    data.insert("parameters".into(), parameters.clone());
    let created_at_str = created_at.map(|ca| ca.to_rfc3339());
    data.insert("createdAt".into(), json!(created_at_str));

    match share_type {
        4 => {
            data.insert("boardId".into(), json!(entity_id));
            let board_params = sqlx::query_scalar::<_, Value>(
                r#"SELECT parameters FROM "board" WHERE board_id = $1"#,
            )
            .bind(entity_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| json!({}));

            let mut website_ids: Vec<String> = Vec::new();
            let mut pixel_ids: Vec<String> = Vec::new();
            let mut link_ids: Vec<String> = Vec::new();

            if let Some(params_obj) = board_params.as_object() {
                if let Some(wid) = params_obj.get("websiteId").and_then(Value::as_str) {
                    website_ids.push(wid.to_string());
                }
                if let Some(pid) = params_obj.get("pixelId").and_then(Value::as_str) {
                    pixel_ids.push(pid.to_string());
                }
                if let Some(lid) = params_obj.get("linkId").and_then(Value::as_str) {
                    link_ids.push(lid.to_string());
                }
                if let Some(rows) = params_obj.get("rows").and_then(Value::as_array) {
                    for r in rows {
                        if let Some(cols) = r.get("columns").and_then(Value::as_array) {
                            for col in cols {
                                if let Some(comp) = col.get("component").and_then(Value::as_object)
                                {
                                    let etype = comp
                                        .get("entityType")
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    if let Some(eid) = comp.get("entityId").and_then(Value::as_str)
                                    {
                                        match etype {
                                            "website" => website_ids.push(eid.to_string()),
                                            "pixel" => pixel_ids.push(eid.to_string()),
                                            "link" => link_ids.push(eid.to_string()),
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            website_ids.sort();
            website_ids.dedup();
            pixel_ids.sort();
            pixel_ids.dedup();
            link_ids.sort();
            link_ids.dedup();

            data.insert("websiteIds".into(), json!(website_ids));
            data.insert("pixelIds".into(), json!(pixel_ids));
            data.insert("linkIds".into(), json!(link_ids));
        }
        1 => {
            data.insert("websiteId".into(), json!(entity_id));
        }
        2 => {
            data.insert("websiteId".into(), json!(entity_id));
            data.insert("linkId".into(), json!(entity_id));
        }
        3 => {
            data.insert("websiteId".into(), json!(entity_id));
            data.insert("pixelId".into(), json!(entity_id));
        }
        _ => {}
    }

    let mut jwt_claims = data.clone();
    jwt_claims.insert("type".into(), json!("share"));
    let exp = (chrono::Utc::now() + chrono::Duration::days(3650)).timestamp();
    jwt_claims.insert("exp".into(), json!(exp));

    let secret = std::env::var("APP_SECRET").unwrap_or_else(|_| "kombu-secret".into());
    let token = crate::auth::encode_jwt_token(&jwt_claims, &secret).unwrap_or_default();

    data.insert("token".into(), json!(token));

    Ok(Json(Value::Object(data)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_slug_generator() {
        let s8 = generate_slug(8);
        assert_eq!(s8.len(), 8);
        let s20 = generate_slug(20);
        assert_eq!(s20.len(), 20);
    }

    #[tokio::test]
    async fn test_share_full_lifecycle() {
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

        let entity_id = Uuid::now_v7();

        let res_no_eid = create(State(state.clone()), Json(json!({}))).await;
        assert!(res_no_eid.is_err());
        assert_eq!(res_no_eid.unwrap_err().0, StatusCode::BAD_REQUEST);

        let initial_slug = format!("slug_{}", Uuid::now_v7().simple());
        let res_create = create(
            State(state.clone()),
            Json(json!({
                "entityId": entity_id.to_string(),
                "shareType": 1,
                "name": "Initial Share",
                "slug": initial_slug
            })),
        )
        .await
        .unwrap();
        let share_id_str = res_create.0["id"].as_str().unwrap();
        let share_id = Uuid::parse_str(share_id_str).unwrap();
        assert_eq!(res_create.0["name"], "Initial Share");

        let res_dup_slug = create(
            State(state.clone()),
            Json(json!({
                "entityId": entity_id.to_string(),
                "shareType": 1,
                "name": "Dup Share",
                "slug": initial_slug
            })),
        )
        .await;
        assert!(res_dup_slug.is_err());
        assert_eq!(res_dup_slug.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_slash = create(
            State(state.clone()),
            Json(json!({
                "entityId": entity_id.to_string(),
                "slug": "bad/slug"
            })),
        )
        .await;
        assert!(res_slash.is_err());
        assert_eq!(res_slash.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_too_long = create(
            State(state.clone()),
            Json(json!({
                "entityId": entity_id.to_string(),
                "slug": "a".repeat(101)
            })),
        )
        .await;
        assert!(res_too_long.is_err());
        assert_eq!(res_too_long.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_list = list(State(state.clone())).await.unwrap();
        assert!(!res_list.0.as_array().unwrap().is_empty());

        let res_entity_shares = get_entity_shares(Path(entity_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_entity_shares.0["count"], 1);

        let res_get_id = get_by_id(Path(share_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_get_id.0["name"], "Initial Share");

        let res_get_id_404 = get_by_id(Path(Uuid::now_v7()), State(state.clone())).await;
        assert!(res_get_id_404.is_err());
        assert_eq!(res_get_id_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_empty_slug = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({ "slug": "   " })),
        )
        .await;
        assert!(res_empty_slug.is_err());
        assert_eq!(res_empty_slug.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_slash_slug = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({ "slug": "bad/slug" })),
        )
        .await;
        assert!(res_slash_slug.is_err());
        assert_eq!(res_slash_slug.unwrap_err().0, StatusCode::BAD_REQUEST);

        let long_slug = "a".repeat(101);
        let res_long_slug = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({ "slug": long_slug })),
        )
        .await;
        assert!(res_long_slug.is_err());
        assert_eq!(res_long_slug.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_ghost_up = update_by_id(
            Path(Uuid::now_v7()),
            State(state.clone()),
            Json(json!({ "name": "new" })),
        )
        .await;
        assert!(res_ghost_up.is_err());
        assert_eq!(res_ghost_up.unwrap_err().0, StatusCode::NOT_FOUND);

        let other_slug = format!("other_slug_{}", Uuid::now_v7().simple());
        let _ = create(
            State(state.clone()),
            Json(json!({
                "entityId": entity_id.to_string(),
                "shareType": 1,
                "name": "Other Share",
                "slug": other_slug
            })),
        )
        .await;

        let res_up_conflict = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({ "slug": other_slug })),
        )
        .await;
        assert!(res_up_conflict.is_err());
        assert_eq!(res_up_conflict.unwrap_err().0, StatusCode::BAD_REQUEST);

        let updated_slug = format!("new_slug_{}", Uuid::now_v7().simple());
        let res_up = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({
                "name": "Renamed Share",
                "slug": updated_slug,
                "parameters": { "custom": "param" }
            })),
        )
        .await
        .unwrap();
        assert_eq!(res_up.0["name"], "Renamed Share");
        assert_eq!(res_up.0["slug"], updated_slug);

        let res_up_preserve_aliases = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({
                "parameters": { "custom": "param2" }
            })),
        )
        .await
        .unwrap();
        assert!(res_up_preserve_aliases.0["parameters"]["aliases"].is_array());

        let updated_slug_3 = format!("third_slug_{}", Uuid::now_v7().simple());
        let res_up_third = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({
                "slug": updated_slug_3,
                "parameters": { "custom": "param3" }
            })),
        )
        .await
        .unwrap();
        assert_eq!(res_up_third.0["slug"], updated_slug_3);

        let res_up_same = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({ "slug": updated_slug_3 })),
        )
        .await
        .unwrap();
        assert_eq!(res_up_same.0["slug"], updated_slug_3);

        let _ = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({
                "parameters": { "aliases": [initial_slug.clone(), updated_slug_3.clone()] }
            })),
        )
        .await;

        let res_up_back = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({ "slug": updated_slug })),
        )
        .await
        .unwrap();
        assert_eq!(res_up_back.0["slug"], updated_slug);

        let non_obj_share_id = Uuid::now_v7();
        let non_obj_slug = format!("non_obj_{}", non_obj_share_id.simple());
        let _ = sqlx::query(
            r#"INSERT INTO "share" (share_id, entity_id, name, share_type, slug, parameters, created_at, updated_at) VALUES ($1, $2, 'Non-obj', 1, $3, '"string_param"', NOW(), NOW())"#
        )
        .bind(non_obj_share_id)
        .bind(entity_id)
        .bind(&non_obj_slug)
        .execute(&pool)
        .await;

        let non_obj_new_slug = format!("non_obj_new_{}", non_obj_share_id.simple());
        let res_up_non_obj = update_by_id(
            Path(non_obj_share_id),
            State(state.clone()),
            Json(json!({ "slug": non_obj_new_slug })),
        )
        .await
        .unwrap();
        assert_eq!(res_up_non_obj.0["slug"], non_obj_new_slug);

        let too_long_name = "a".repeat(250);
        let res_up_db_err = update_by_id(
            Path(share_id),
            State(state.clone()),
            Json(json!({ "name": too_long_name })),
        )
        .await;
        assert!(res_up_db_err.is_err());
        assert_eq!(
            res_up_db_err.unwrap_err().0,
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let res_cr_db_err = create(
            State(state.clone()),
            Json(json!({
                "entityId": entity_id.to_string(),
                "shareType": 1,
                "name": too_long_name
            })),
        )
        .await;
        assert!(res_cr_db_err.is_err());
        assert_eq!(
            res_cr_db_err.unwrap_err().0,
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let res_by_new_slug = get_by_slug(Path(updated_slug.clone()), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_by_new_slug.0["name"], "Renamed Share");
        assert!(res_by_new_slug.0["token"].is_string());

        let res_by_alias = get_by_slug(Path(initial_slug), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_by_alias.0["name"], "Renamed Share");

        let b_id = Uuid::now_v7();
        let board_params = json!({
            "websiteId": "w1",
            "pixelId": "p1",
            "linkId": "l1",
            "rows": [
                {
                    "columns": [
                        { "component": { "entityType": "website", "entityId": "w2" } },
                        { "component": { "entityType": "pixel", "entityId": "p2" } },
                        { "component": { "entityType": "link", "entityId": "l2" } },
                        { "component": { "entityType": "unknown", "entityId": "u1" } },
                        { "component": { "entityType": "website" } },
                        { "component": null },
                        {}
                    ]
                },
                {}
            ]
        });
        let _ = sqlx::query(r#"INSERT INTO "board" (board_id, name, description, type, parameters, created_at, updated_at) VALUES ($1, 'Test Board', '', 'default', $2, NOW(), NOW())"#)
            .bind(b_id)
            .bind(&board_params)
            .execute(&pool)
            .await;

        let b_slug = format!("b_slug_{}", Uuid::now_v7().simple());
        let _ = create_board_share(
            Path(b_id),
            State(state.clone()),
            Json(json!({ "name": "Board Share", "slug": b_slug })),
        )
        .await
        .unwrap();
        let res_b_slug = get_by_slug(Path(b_slug), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_b_slug.0["boardId"], b_id.to_string());
        assert_eq!(res_b_slug.0["websiteIds"], json!(["w1", "w2"]));
        assert_eq!(res_b_slug.0["pixelIds"], json!(["p1", "p2"]));
        assert_eq!(res_b_slug.0["linkIds"], json!(["l1", "l2"]));

        let b2_id = Uuid::now_v7();
        let _ = sqlx::query(r#"INSERT INTO "board" (board_id, name, description, type, parameters, created_at, updated_at) VALUES ($1, 'Empty Board', '', 'default', '{}', NOW(), NOW())"#)
            .bind(b2_id)
            .execute(&pool)
            .await;
        let b2_slug = format!("b2_slug_{}", Uuid::now_v7().simple());
        let _ = create_board_share(
            Path(b2_id),
            State(state.clone()),
            Json(json!({ "name": "Empty Board Share", "slug": b2_slug })),
        )
        .await
        .unwrap();
        let res_b2_slug = get_by_slug(Path(b2_slug), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_b2_slug.0["boardId"], b2_id.to_string());

        let b3_id = Uuid::now_v7();
        let _ = sqlx::query(r#"INSERT INTO "board" (board_id, name, description, type, parameters, created_at, updated_at) VALUES ($1, 'String Board', '', 'default', '"string_parameters"', NOW(), NOW())"#)
            .bind(b3_id)
            .execute(&pool)
            .await;
        let b3_slug = format!("b3_slug_{}", Uuid::now_v7().simple());
        let _ = create_board_share(
            Path(b3_id),
            State(state.clone()),
            Json(json!({ "name": "String Board Share", "slug": b3_slug })),
        )
        .await
        .unwrap();
        let res_b3_slug = get_by_slug(Path(b3_slug), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_b3_slug.0["boardId"], b3_id.to_string());

        let w_id = Uuid::now_v7();
        let _ = create_website_share(
            Path(w_id),
            State(state.clone()),
            Json(json!({ "name": "Website Share" })),
        )
        .await
        .unwrap();

        let l_id = Uuid::now_v7();
        let l_slug = format!("l_slug_{}", Uuid::now_v7().simple());
        let _ = create_link_share(
            Path(l_id),
            State(state.clone()),
            Json(json!({ "name": "Link Share", "slug": l_slug })),
        )
        .await
        .unwrap();
        let res_l_slug = get_by_slug(Path(l_slug), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_l_slug.0["linkId"], l_id.to_string());

        let p_id = Uuid::now_v7();
        let p_slug = format!("p_slug_{}", Uuid::now_v7().simple());
        let _ = create_pixel_share(
            Path(p_id),
            State(state.clone()),
            Json(json!({ "name": "Pixel Share", "slug": p_slug })),
        )
        .await
        .unwrap();
        let res_p_slug = get_by_slug(Path(p_slug), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_p_slug.0["pixelId"], p_id.to_string());

        let other_share_id = Uuid::now_v7();
        let other_type_slug = format!("other_type_{}", other_share_id.simple());
        let _ = sqlx::query(r#"INSERT INTO "share" (share_id, entity_id, name, share_type, slug, parameters, created_at, updated_at) VALUES ($1, $2, 'Other Type', 99, $3, '{}', NOW(), NOW())"#)
            .bind(other_share_id)
            .bind(Uuid::now_v7())
            .bind(&other_type_slug)
            .execute(&pool)
            .await;
        let res_other_slug = get_by_slug(Path(other_type_slug), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_other_slug.0["shareType"], 99);

        let res_slug_404 = get_by_slug(
            Path("nonexistent_share_slug_xyz".into()),
            State(state.clone()),
        )
        .await;
        assert!(res_slug_404.is_err());
        assert_eq!(res_slug_404.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_del = delete_by_id(Path(share_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_del.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "board" WHERE board_id = $1"#)
            .bind(b_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(
            r#"DELETE FROM "share" WHERE entity_id IN ($1, $2, $3, $4, $5) OR share_id = $6"#,
        )
        .bind(entity_id)
        .bind(b_id)
        .bind(w_id)
        .bind(l_id)
        .bind(p_id)
        .bind(other_share_id)
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

        assert!(list(State(err_state.clone())).await.is_err());
        assert!(
            get_entity_shares(Path(entity_id), State(err_state.clone()))
                .await
                .is_ok()
        );
        assert!(
            create(
                State(err_state.clone()),
                Json(json!({ "entityId": entity_id.to_string(), "name": "Fail" }))
            )
            .await
            .is_err()
        );
        assert!(
            get_by_id(Path(share_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            update_by_id(
                Path(share_id),
                State(err_state.clone()),
                Json(json!({ "name": "Fail" }))
            )
            .await
            .is_err()
        );
        assert!(
            delete_by_id(Path(share_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            get_by_slug(Path("any".into()), State(err_state.clone()))
                .await
                .is_err()
        );
    }
}
