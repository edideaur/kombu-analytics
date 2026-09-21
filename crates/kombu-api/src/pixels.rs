#![forbid(unsafe_code)]

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::router::AppState;

const PIXEL_GIF: [u8; 43] = [
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
];

pub const MAX_IMAGE_SIZE: usize = 512 * 1024;

fn detect_image_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"<svg")
        || (bytes.starts_with(b"<?xml") && bytes.windows(4).any(|w| w == b"<svg"))
    {
        Some("image/svg+xml")
    } else {
        None
    }
}

type PixelHeaders = [(header::HeaderName, String); 3];

pub async fn render_pixel(
    Path(slug): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, PixelHeaders, &'static str)> {
    let row = sqlx::query_as::<_, (Uuid, Option<Vec<u8>>, Option<String>)>(
        r#"SELECT pixel_id, image_data, content_type FROM "pixel" WHERE slug = $1 AND deleted_at IS NULL"#,
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some((pixel_id, image_data, content_type)) = row else {
        let response_headers = [
            (
                header::CACHE_CONTROL,
                "max-age=0, must-revalidate, no-cache, no-store, private".to_string(),
            ),
            (header::PRAGMA, "no-cache".to_string()),
            (header::EXPIRES, "0".to_string()),
        ];
        return Err((StatusCode::NOT_FOUND, response_headers, "Pixel not found"));
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
    let slug_clone = slug.clone();

    tokio::spawn(async move {
        let created_at = Utc::now();
        let session_id =
            kombu_ingest::generate_session_id(pixel_id, &ip, &user_agent, "secret", created_at);
        let visit_id = kombu_ingest::generate_visit_id(session_id, created_at);
        let payload = kombu_ingest::CollectData {
            website: None,
            link: None,
            pixel: Some(pixel_id.to_string()),
            hostname: None,
            language: None,
            referrer,
            screen: None,
            title: None,
            url: Some(format!("/p/{slug_clone}")),
            name: None,
            data: None,
            tag: None,
            ip: Some(ip),
            user_agent: Some(user_agent),
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
            event_type: Some(kombu_core::constants::EVENT_TYPE_PIXEL_EVENT),
            ..Default::default()
        };
        let _ = kombu_ingest::save_session_and_event(
            &pool, pixel_id, session_id, visit_id, &payload, created_at, "desktop", None, None,
            None, None, None,
        )
        .await;
    });

    let (ct, body_bytes) = match (image_data, content_type) {
        (Some(data), Some(content_type)) => (content_type, data),
        _ => ("image/gif".to_string(), PIXEL_GIF.to_vec()),
    };

    let response_headers = [
        (header::CONTENT_TYPE, ct),
        (
            header::CACHE_CONTROL,
            "max-age=0, must-revalidate, no-cache, no-store, private".to_string(),
        ),
        (header::PRAGMA, "no-cache".to_string()),
        (header::EXPIRES, "0".to_string()),
    ];

    Ok((response_headers, body_bytes))
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM (
            SELECT
                pixel_id as id,
                name,
                slug,
                user_id as "userId",
                team_id as "teamId",
                (image_data IS NOT NULL) as "hasCustomImage",
                content_type as "contentType",
                created_at as "createdAt"
            FROM "pixel"
            WHERE deleted_at IS NULL
            ORDER BY created_at DESC
        ) t
        "#,
    )
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

pub async fn create(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pixel_id = Uuid::now_v7();
    let name = body["name"].as_str().unwrap_or("Pixel");
    let slug = body["slug"].as_str().unwrap_or("pixel").trim();
    if slug.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Slug cannot be empty" })),
        ));
    }
    let team_id = body["teamId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());

    let (image_data, content_type) = parse_image_from_body(&body)?;

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
        INSERT INTO "pixel" (pixel_id, name, slug, user_id, team_id, image_data, content_type, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        "#,
    )
    .bind(pixel_id)
    .bind(name)
    .bind(slug)
    .bind(assigned_user_id)
    .bind(team_id)
    .bind(image_data)
    .bind(content_type)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    get(Path(pixel_id), State(state)).await
}

pub async fn get(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(t) FROM (
            SELECT
                pixel_id as id,
                name,
                slug,
                user_id as "userId",
                team_id as "teamId",
                (image_data IS NOT NULL) as "hasCustomImage",
                content_type as "contentType",
                created_at as "createdAt"
            FROM "pixel"
            WHERE pixel_id = $1 AND deleted_at IS NULL
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
        Some(p) => Ok(Json(p)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Pixel not found" })),
        )),
    }
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body["name"].as_str();
    let slug = body["slug"].as_str().map(str::trim);

    if let Some(s) = slug {
        if s.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Slug cannot be empty" })),
            ));
        }
    }

    let has_image_field = body.get("image").is_some() || body.get("imageData").is_some();
    let (custom_image, custom_ct) = if has_image_field {
        parse_image_from_body(&body)?
    } else {
        (None, None)
    };

    if has_image_field {
        sqlx::query(
            r#"
            UPDATE "pixel"
            SET
                name = COALESCE($2, name),
                slug = COALESCE($3, slug),
                image_data = $4,
                content_type = $5,
                updated_at = NOW()
            WHERE pixel_id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(custom_image)
        .bind(custom_ct)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    } else {
        sqlx::query(
            r#"
            UPDATE "pixel"
            SET
                name = COALESCE($2, name),
                slug = COALESCE($3, slug),
                updated_at = NOW()
            WHERE pixel_id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(slug)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    }

    get(Path(id), State(state)).await
}

pub async fn upload_image(
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if body.len() > MAX_IMAGE_SIZE {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": "Image file too large (max 512 KiB)" })),
        ));
    }

    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Empty image payload" })),
        ));
    }

    let detected_ct = detect_image_content_type(&body);
    let header_ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|ct| {
            let media = ct.split(';').next().unwrap_or(ct).trim().to_lowercase();
            if media.starts_with("image/") {
                Some(media)
            } else {
                None
            }
        });

    let content_type = detected_ct
        .map(str::to_string)
        .or(header_ct)
        .unwrap_or_else(|| "image/png".to_string());

    sqlx::query(
        r#"
        UPDATE "pixel"
        SET
            image_data = $2,
            content_type = $3,
            updated_at = NOW()
        WHERE pixel_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(body.as_ref())
    .bind(&content_type)
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

pub async fn delete_image(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(
        r#"
        UPDATE "pixel"
        SET
            image_data = NULL,
            content_type = NULL,
            updated_at = NOW()
        WHERE pixel_id = $1 AND deleted_at IS NULL
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

    get(Path(id), State(state)).await
}

pub async fn delete(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    sqlx::query(r#"UPDATE "pixel" SET deleted_at = NOW() WHERE pixel_id = $1"#)
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

type ParsedImageResult = Result<(Option<Vec<u8>>, Option<String>), (StatusCode, Json<Value>)>;

fn parse_image_from_body(body: &Value) -> ParsedImageResult {
    let img_val = body.get("image").or_else(|| body.get("imageData"));

    let Some(val) = img_val else {
        return Ok((None, None));
    };

    if val.is_null() {
        return Ok((None, None));
    }

    let Some(raw_str) = val.as_str() else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Image must be base64 string or null" })),
        ));
    };

    let trimmed = raw_str.trim();
    if trimmed.is_empty() {
        return Ok((None, None));
    }

    let (b64_part, explicit_ct) = if let Some(stripped) = trimmed.strip_prefix("data:") {
        if let Some((header_part, data_part)) = stripped.split_once(',') {
            let parsed_ct = header_part.split(';').next().unwrap_or("").trim();
            let ct = if parsed_ct.is_empty() {
                None
            } else {
                Some(parsed_ct.to_string())
            };
            (data_part.trim(), ct)
        } else {
            (trimmed, None)
        }
    } else {
        (trimmed, None)
    };

    let decoded = BASE64_STANDARD.decode(b64_part).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid base64 image data: {e}") })),
        )
    })?;

    if decoded.len() > MAX_IMAGE_SIZE {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "error": "Image file too large (max 512 KiB)" })),
        ));
    }

    let ct = explicit_ct
        .or_else(|| detect_image_content_type(&decoded).map(str::to_string))
        .unwrap_or_else(|| "image/png".into());

    Ok((Some(decoded), Some(ct)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_detect_image_content_type() {
        assert_eq!(
            detect_image_content_type(b"\x89PNG\r\n\x1a\n"),
            Some("image/png")
        );
        assert_eq!(detect_image_content_type(b"GIF87a"), Some("image/gif"));
        assert_eq!(detect_image_content_type(b"GIF89a"), Some("image/gif"));
        assert_eq!(
            detect_image_content_type(b"\xff\xd8\xff\xe0"),
            Some("image/jpeg")
        );
        let webp_header = b"RIFF1234WEBP";
        assert_eq!(detect_image_content_type(webp_header), Some("image/webp"));
        assert_eq!(
            detect_image_content_type(b"<svg viewBox='0 0 10 10'></svg>"),
            Some("image/svg+xml")
        );
        assert_eq!(
            detect_image_content_type(b"<?xml version=\"1.0\"?><svg viewBox='0 0 10 10'></svg>"),
            Some("image/svg+xml")
        );
        assert_eq!(detect_image_content_type(b"random text"), None);
    }

    #[test]
    fn test_parse_image_from_body() {
        assert_eq!(parse_image_from_body(&json!({})).unwrap(), (None, None));
        assert_eq!(
            parse_image_from_body(&json!({ "image": null })).unwrap(),
            (None, None)
        );
        assert!(parse_image_from_body(&json!({ "image": 1234 })).is_err());
        assert_eq!(
            parse_image_from_body(&json!({ "image": "   " })).unwrap(),
            (None, None)
        );

        let encoded_gif = BASE64_STANDARD.encode(PIXEL_GIF);
        let data_url = format!("data:image/gif;base64,{encoded_gif}");
        let (data, ct) = parse_image_from_body(&json!({ "image": data_url })).unwrap();
        assert_eq!(data, Some(PIXEL_GIF.to_vec()));
        assert_eq!(ct, Some("image/gif".to_string()));

        let data_url_no_type = format!("data:;base64,{encoded_gif}");
        let (data_nt, ct_nt) =
            parse_image_from_body(&json!({ "image": data_url_no_type })).unwrap();
        assert_eq!(data_nt, Some(PIXEL_GIF.to_vec()));
        assert_eq!(ct_nt, Some("image/gif".to_string()));

        let raw_unknown = BASE64_STANDARD.encode(b"UNKNOWN_BINARY_DATA_FOR_IMG");
        let (data_un, ct_un) = parse_image_from_body(&json!({ "image": raw_unknown })).unwrap();
        assert_eq!(data_un, Some(b"UNKNOWN_BINARY_DATA_FOR_IMG".to_vec()));
        assert_eq!(ct_un, Some("image/png".into()));

        assert!(parse_image_from_body(&json!({ "image": "not!base64???" })).is_err());
        assert!(parse_image_from_body(&json!({ "image": "data:no_comma_part" })).is_err());

        let oversized = BASE64_STANDARD.encode(vec![0u8; MAX_IMAGE_SIZE + 10]);
        let err_large = parse_image_from_body(&json!({ "image": oversized }));
        assert!(err_large.is_err());
        assert_eq!(err_large.unwrap_err().0, StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_pixel_full_crud_and_render() {
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

        let res_bad_slug = create(
            HeaderMap::new(),
            State(state.clone()),
            Json(json!({ "name": "Test", "slug": "  " })),
        )
        .await;
        assert!(res_bad_slug.is_err());
        assert_eq!(res_bad_slug.unwrap_err().0, StatusCode::BAD_REQUEST);

        let slug = format!("px-{}", Uuid::now_v7().simple());
        let res_create = create(
            HeaderMap::new(),
            State(state.clone()),
            Json(json!({
                "name": "My Tracking Pixel",
                "slug": slug.clone()
            })),
        )
        .await
        .unwrap();
        let pixel_id_str = res_create.0["id"].as_str().unwrap();
        let pixel_id = Uuid::parse_str(pixel_id_str).unwrap();

        let res_create_bad_img = create(
            HeaderMap::new(),
            State(state.clone()),
            Json(json!({
                "name": "Bad Img Pixel",
                "slug": format!("bad_img_{}", Uuid::now_v7()),
                "image": "not!valid!base64"
            })),
        )
        .await;
        assert!(res_create_bad_img.is_err());

        let team_id = Uuid::now_v7();
        let _ = sqlx::query(r#"INSERT INTO "team" (team_id, name, access_code) VALUES ($1, 'Pixel Team', 'code_px')"#)
            .bind(team_id)
            .execute(&pool)
            .await;
        let team_slug = format!("px-team-{}", Uuid::now_v7().simple());
        let res_team_create = create(
            HeaderMap::new(),
            State(state.clone()),
            Json(json!({
                "name": "Team Pixel",
                "slug": team_slug,
                "teamId": team_id
            })),
        )
        .await;
        assert!(res_team_create.is_ok());
        let team_px_id: Uuid = res_team_create.unwrap().0["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let _ = delete(Path(team_px_id), State(state.clone())).await;
        let _ = sqlx::query(r#"DELETE FROM "team" WHERE team_id = $1"#)
            .bind(team_id)
            .execute(&pool)
            .await;

        let res_list = list(State(state.clone())).await.unwrap();
        assert!(res_list.0["count"].as_i64().unwrap() >= 1);

        let res_get = get(Path(pixel_id), State(state.clone())).await.unwrap();
        assert_eq!(res_get.0["name"], "My Tracking Pixel");

        let res_get_none = get(Path(Uuid::now_v7()), State(state.clone())).await;
        assert!(res_get_none.is_err());
        assert_eq!(res_get_none.unwrap_err().0, StatusCode::NOT_FOUND);

        let res_up_bad = update(
            Path(pixel_id),
            State(state.clone()),
            Json(json!({ "slug": "   " })),
        )
        .await;
        assert!(res_up_bad.is_err());

        let res_up_bad_img = update(
            Path(pixel_id),
            State(state.clone()),
            Json(json!({ "image": "not!valid!base64" })),
        )
        .await;
        assert!(res_up_bad_img.is_err());
        assert_eq!(res_up_bad.unwrap_err().0, StatusCode::BAD_REQUEST);

        let new_slug = format!("renamed-{}", Uuid::now_v7().simple());
        let res_up_slug = update(
            Path(pixel_id),
            State(state.clone()),
            Json(json!({ "slug": new_slug })),
        )
        .await
        .unwrap();
        assert_eq!(res_up_slug.0["slug"], new_slug);

        let res_up = update(
            Path(pixel_id),
            State(state.clone()),
            Json(json!({ "name": "Updated Pixel" })),
        )
        .await
        .unwrap();
        assert_eq!(res_up.0["name"], "Updated Pixel");

        let res_up_empty = upload_image(
            Path(pixel_id),
            HeaderMap::new(),
            State(state.clone()),
            Bytes::new(),
        )
        .await;
        assert!(res_up_empty.is_err());
        assert_eq!(res_up_empty.unwrap_err().0, StatusCode::BAD_REQUEST);

        let res_up_large = upload_image(
            Path(pixel_id),
            HeaderMap::new(),
            State(state.clone()),
            Bytes::from(vec![0u8; MAX_IMAGE_SIZE + 1]),
        )
        .await;
        assert!(res_up_large.is_err());
        assert_eq!(res_up_large.unwrap_err().0, StatusCode::PAYLOAD_TOO_LARGE);

        let png_bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15c4\x00\x00\x00\nIDATx\x9cc\x00\x01\x00\x00\x05\x00\x01\r\n-\xb4\x00\x00\x00\x00IEND\xaeB`\x82";
        let mut upload_headers = HeaderMap::new();
        upload_headers.insert("content-type", HeaderValue::from_static("image/png"));
        let res_upload_ok = upload_image(
            Path(pixel_id),
            upload_headers,
            State(state.clone()),
            Bytes::from_static(png_bytes),
        )
        .await
        .unwrap();
        assert_eq!(res_upload_ok.0["hasCustomImage"], true);

        let mut non_img_headers = HeaderMap::new();
        non_img_headers.insert("content-type", HeaderValue::from_static("text/plain"));
        let res_upload_non_img = upload_image(
            Path(pixel_id),
            non_img_headers,
            State(state.clone()),
            Bytes::from_static(b"not-an-image-data"),
        )
        .await;
        assert!(res_upload_non_img.is_ok());

        let mut req_headers = HeaderMap::new();
        req_headers.insert(
            "user-agent",
            HeaderValue::from_static("PixelTestBrowser/1.0"),
        );
        req_headers.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
        req_headers.insert(
            "referer",
            HeaderValue::from_static("https://pixelref.example.com"),
        );
        let rendered = render_pixel(
            Path(new_slug.clone()),
            req_headers.clone(),
            State(state.clone()),
        )
        .await
        .unwrap();
        let resp = rendered.into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        let res_del_img = delete_image(Path(pixel_id), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res_del_img.0["hasCustomImage"], false);

        let rendered_default =
            render_pixel(Path(new_slug.clone()), req_headers, State(state.clone()))
                .await
                .unwrap();
        let resp_default = rendered_default.into_response();
        assert_eq!(resp_default.status(), StatusCode::OK);

        let rendered_no_headers =
            render_pixel(Path(new_slug), HeaderMap::new(), State(state.clone()))
                .await
                .unwrap();
        let resp_no_headers = rendered_no_headers.into_response();
        assert_eq!(resp_no_headers.status(), StatusCode::OK);

        let render_missing = render_pixel(
            Path("nonexistent-pixel-slug".into()),
            HeaderMap::new(),
            State(state.clone()),
        )
        .await;
        let (status, _, _) = render_missing.err().unwrap();
        assert_eq!(status, StatusCode::NOT_FOUND);

        let res_del = delete(Path(pixel_id), State(state.clone())).await.unwrap();
        assert_eq!(res_del.0["ok"], true);

        let _ = sqlx::query(r#"DELETE FROM "website_event" WHERE website_id = $1"#)
            .bind(pixel_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "session" WHERE website_id = $1"#)
            .bind(pixel_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query(r#"DELETE FROM "pixel" WHERE pixel_id = $1"#)
            .bind(pixel_id)
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

        assert!(list(State(err_state.clone())).await.is_ok());
        assert!(get(Path(pixel_id), State(err_state.clone())).await.is_err());
        assert!(
            create(
                HeaderMap::new(),
                State(err_state.clone()),
                Json(json!({ "name": "Fail", "slug": "px-fail" }))
            )
            .await
            .is_err()
        );
        assert!(
            update(
                Path(pixel_id),
                State(err_state.clone()),
                Json(json!({ "name": "Fail" }))
            )
            .await
            .is_err()
        );
        assert!(update(Path(pixel_id), State(err_state.clone()), Json(json!({ "name": "Fail", "image": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=" }))).await.is_err());
        assert!(
            upload_image(
                Path(pixel_id),
                HeaderMap::new(),
                State(err_state.clone()),
                Bytes::from_static(b"image-bytes")
            )
            .await
            .is_err()
        );
        assert!(
            delete_image(Path(pixel_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            delete(Path(pixel_id), State(err_state.clone()))
                .await
                .is_err()
        );
        assert!(
            render_pixel(
                Path("any".into()),
                HeaderMap::new(),
                State(err_state.clone())
            )
            .await
            .is_err()
        );
    }
}
