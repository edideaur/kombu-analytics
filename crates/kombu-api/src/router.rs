#![forbid(unsafe_code)]

use axum::{
    Router,
    response::IntoResponse,
    routing::{get, post},
};
use quick_cache::sync::Cache;
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use crate::{
    admin, alerts, annotations, auth, boards, config, dashboard, event_data, events,
    export_handler, ingest, links, me, pixels, queue::IngestQueue, realtime, recorder, reports,
    retention, revenue, segments, session_data, sessions, share, teams, two_factor, users,
    websites,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub website_cache: Arc<Cache<Uuid, bool>>,
    pub session_cache: Arc<Cache<Uuid, bool>>,
    pub ingest_queue: IngestQueue,
    pub app_secret: Arc<String>,
    pub rate_limiter: crate::rate_limit::RateLimiter,
}

#[derive(Clone, Copy)]
struct SecurityHeadersConfig {
    force_ssl: bool,
}

pub fn build_router(pool: PgPool) -> Router {
    build_router_with_lookup(pool, &|k| std::env::var(k).ok())
}

pub fn build_router_with_lookup(pool: PgPool, get_env: &dyn Fn(&str) -> Option<String>) -> Router {
    let app_secret = Arc::new(
        get_env("APP_SECRET")
            .or_else(|| get_env("HASH_SALT"))
            .unwrap_or_else(|| "kombu-secret".into()),
    );
    let ingest_queue = IngestQueue::new(&pool, 1_000_000, 32);
    let state = AppState {
        pool,
        website_cache: Arc::new(Cache::new(100_000)),
        session_cache: Arc::new(Cache::new(1_000_000)),
        ingest_queue,
        app_secret,
        rate_limiter: crate::rate_limit::RateLimiter::new(),
    };
    let mut router = Router::new()
        .route("/api/config", get(config::get))
        .route("/api/dashboard", get(dashboard::get).post(dashboard::save))
        .route("/api/dashboard/overview", get(dashboard::overview))
        .route("/api/websites/overview", get(dashboard::overview))
        .route("/api/me", get(me::get))
        .route("/api/me/websites", get(me::websites))
        .route("/api/me/teams", get(me::teams))
        .route("/api/me/password", post(me::password))
        .route("/api/users", get(users::list).post(users::create))
        .route(
            "/api/users/{id}",
            get(users::get).post(users::update).delete(users::delete),
        )
        .route("/api/users/{id}/websites", get(users::websites))
        .route("/api/users/{id}/teams", get(users::teams))
        .route("/api/teams", get(teams::list).post(teams::create))
        .route("/api/teams/join", post(teams::join))
        .route(
            "/api/teams/{id}",
            get(teams::get).post(teams::update).delete(teams::delete),
        )
        .route(
            "/api/teams/{id}/users",
            get(teams::users).post(teams::add_user),
        )
        .route(
            "/api/teams/{id}/users/{userId}",
            axum::routing::delete(teams::delete_user),
        )
        .route("/api/teams/{id}/websites", get(teams::websites))
        .route("/api/teams/{id}/boards", get(teams::boards))
        .route("/api/teams/{id}/pixels", get(teams::pixels))
        .route("/api/teams/{id}/links", get(teams::links))
        .route(
            "/api/teams/{id}/invitations",
            get(teams::list_invitations).post(teams::create_invitation),
        )
        .route(
            "/api/teams/{id}/invitations/{invitationId}",
            axum::routing::delete(teams::revoke_invitation),
        )
        .route(
            "/api/teams/invitations/{token}",
            get(teams::get_invitation_by_token),
        )
        .route(
            "/api/teams/invitations/{token}/accept",
            post(teams::accept_invitation_by_token),
        )
        .route("/api/admin/users", get(admin::users))
        .route("/api/admin/teams", get(admin::teams))
        .route("/api/admin/websites", get(admin::websites))
        .route("/api/admin/2fa/global", post(admin::two_factor_global))
        .route(
            "/api/admin/users/{userId}/2fa",
            get(admin::user_two_factor).post(admin::update_user_two_factor),
        )
        .route(
            "/api/admin/teams/{teamId}/2fa",
            get(admin::team_two_factor).post(admin::update_team_two_factor),
        )
        .route(
            "/api/admin/retention/purge",
            post(retention::admin_purge_all),
        )
        .route("/api/send", post(ingest::send))
        .route("/api/batch", post(ingest::batch))
        .route(
            "/api/heartbeat",
            post(ingest::heartbeat_post).get(ingest::heartbeat),
        )
        .route("/api/record", post(recorder::record))
        .route("/script.js", get(tracker_script))
        .route("/umami.js", get(tracker_script))
        .route("/recorder.js", get(recorder_script))
        .route("/telemetry.js", get(telemetry_script))
        .route("/api/scripts/telemetry", get(telemetry_script))
        .route("/q/{slug}", get(links::redirect_slug))
        .route("/p/{slug}", get(pixels::render_pixel))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/verify", post(auth::verify))
        .route("/api/auth/subscription", get(auth::subscription))
        .route("/api/auth/sso", get(auth::sso_config).post(auth::sso))
        .route("/api/auth/sso/login", post(auth::sso))
        .route("/api/auth/sso/callback", get(auth::sso_callback))
        .route("/api/2fa/status", get(two_factor::status))
        .route("/api/2fa/setup/initiate", post(two_factor::initiate))
        .route("/api/2fa/setup/confirm", post(two_factor::confirm))
        .route("/api/2fa/setup/cancel", post(two_factor::cancel))
        .route("/api/2fa/disable", post(two_factor::disable))
        .route("/api/2fa/verify", post(two_factor::verify))
        .route("/api/websites", get(websites::list).post(websites::create))
        .route("/api/websites/charts", get(websites::charts))
        .route(
            "/api/websites/{id}",
            get(websites::get)
                .post(websites::update)
                .delete(websites::delete),
        )
        .route("/api/websites/{id}/stats", get(websites::stats))
        .route("/api/websites/{id}/active", get(websites::active))
        .route("/api/websites/{id}/daterange", get(websites::daterange))
        .route("/api/websites/{id}/metrics", get(websites::metrics))
        .route(
            "/api/websites/{id}/metrics/expanded",
            get(websites::metrics_expanded),
        )
        .route("/api/websites/{id}/pageviews", get(websites::pageviews))
        .route("/api/websites/{id}/values", get(websites::values))
        .route("/api/websites/{id}/reset", post(websites::reset))
        .route("/api/websites/{id}/transfer", post(websites::transfer))
        .route("/api/websites/{id}/entry-exit", get(websites::entry_exit))
        .route("/api/websites/{id}/entry-pages", get(websites::entry_pages))
        .route("/api/websites/{id}/exit-pages", get(websites::exit_pages))
        .route("/api/websites/{id}/engagement", get(websites::engagement))
        .route("/api/websites/{id}/scroll", get(websites::engagement))
        .route("/api/websites/{id}/reports", get(reports::list_for_website))
        .route(
            "/api/websites/{id}/retention",
            get(retention::get_policy).post(retention::save_policy),
        )
        .route("/api/websites/{id}/retention/purge", post(retention::purge))
        .route(
            "/api/websites/{id}/retention/history",
            get(retention::history),
        )
        .route(
            "/api/websites/{id}/shares",
            get(share::get_entity_shares).post(share::create_website_share),
        )
        .route("/api/websites/{id}/events", get(events::list))
        .route("/api/websites/{id}/events/series", get(events::series))
        .route("/api/websites/{id}/events/stats", get(events::stats))
        .route("/api/websites/{id}/sessions", get(sessions::list))
        .route("/api/websites/{id}/sessions/stats", get(sessions::stats))
        .route("/api/websites/{id}/sessions/weekly", get(sessions::weekly))
        .route(
            "/api/websites/{id}/sessions/{sessionId}",
            get(sessions::get),
        )
        .route(
            "/api/websites/{id}/sessions/{sessionId}/activity",
            get(sessions::activity),
        )
        .route(
            "/api/websites/{id}/sessions/{sessionId}/properties",
            get(sessions::properties),
        )
        .route(
            "/api/websites/{id}/sessions/{sessionId}/replays",
            get(recorder::list_session_replays),
        )
        .route("/api/websites/{id}/event-data", get(event_data::list))
        .route(
            "/api/websites/{id}/event-data/properties",
            get(event_data::properties),
        )
        .route(
            "/api/websites/{id}/event-data/values",
            get(event_data::values),
        )
        .route(
            "/api/websites/{id}/event-data/fields",
            get(event_data::fields),
        )
        .route(
            "/api/websites/{id}/event-data/events",
            get(event_data::events),
        )
        .route(
            "/api/websites/{id}/event-data/{eventId}",
            get(event_data::get_by_id),
        )
        .route(
            "/api/websites/{id}/event-data-pivot",
            get(event_data::pivot),
        )
        .route(
            "/api/websites/{id}/event-data-pivot/property-series",
            get(event_data::pivot),
        )
        .route(
            "/api/websites/{id}/event-data-pivot/numeric-stats",
            get(event_data::pivot),
        )
        .route(
            "/api/websites/{id}/event-data-pivot/numeric-series",
            get(event_data::pivot),
        )
        .route(
            "/api/websites/{id}/event-data-pivot/date-series",
            get(event_data::pivot),
        )
        .route(
            "/api/websites/{id}/event-data-pivot/array-series",
            get(event_data::pivot),
        )
        .route(
            "/api/websites/{id}/session-data/properties",
            get(session_data::properties),
        )
        .route(
            "/api/websites/{id}/session-data/values",
            get(session_data::values),
        )
        .route(
            "/api/websites/{id}/session-data/stats",
            get(session_data::stats),
        )
        .route(
            "/api/websites/{id}/session-data-pivot",
            get(session_data::pivot),
        )
        .route(
            "/api/websites/{id}/session-data/property-series",
            get(session_data::pivot),
        )
        .route(
            "/api/websites/{id}/session-data/numeric-stats",
            get(session_data::stats),
        )
        .route(
            "/api/websites/{id}/session-data/numeric-series",
            get(session_data::pivot),
        )
        .route(
            "/api/websites/{id}/session-data/date-series",
            get(session_data::pivot),
        )
        .route(
            "/api/websites/{id}/session-data/array-series",
            get(session_data::pivot),
        )
        .route(
            "/api/websites/{id}/recorder",
            get(recorder::recorder_config),
        )
        .route("/api/websites/{id}/replays", get(recorder::list_replays))
        .route(
            "/api/websites/{id}/replays/saved",
            get(recorder::list_saved_replays),
        )
        .route(
            "/api/websites/{id}/replays/saved/{replayId}",
            get(recorder::get_saved_replay)
                .post(recorder::save_replay)
                .delete(recorder::delete_saved_replay),
        )
        .route(
            "/api/websites/{id}/replays/{replayId}",
            get(recorder::get_replay),
        )
        .route(
            "/api/websites/{id}/segments",
            get(segments::list).post(segments::create),
        )
        .route(
            "/api/websites/{id}/segments/{segmentId}",
            axum::routing::delete(segments::delete),
        )
        .route("/api/websites/{id}/revenue/stats", get(revenue::stats))
        .route("/api/websites/{id}/revenue/chart", get(revenue::chart))
        .route("/api/websites/{id}/revenue/metrics", get(revenue::metrics))
        .route(
            "/api/websites/{id}/revenue/sessions",
            get(revenue::sessions),
        )
        .route(
            "/api/websites/{id}/export",
            get(export_handler::export_events),
        )
        .route(
            "/api/websites/{id}/import",
            post(export_handler::import_events),
        )
        .route("/api/reports", get(reports::list).post(reports::create))
        .route(
            "/api/reports/{id}",
            get(reports::get)
                .post(reports::update)
                .delete(reports::delete),
        )
        .route("/api/reports/funnel", post(reports::run_funnel))
        .route("/api/reports/retention", post(reports::run_retention))
        .route("/api/reports/journey", post(reports::run_journey))
        .route("/api/reports/attribution", post(reports::run_attribution))
        .route("/api/reports/cohorts", post(reports::run_retention))
        .route("/api/reports/revenue", post(reports::run_revenue))
        .route("/api/reports/goal", post(reports::run_goal))
        .route("/api/reports/goals", post(reports::run_goal))
        .route("/api/reports/utm", post(reports::run_utm))
        .route("/api/reports/heatmap", post(reports::run_heatmap))
        .route("/api/reports/performance", post(reports::run_performance))
        .route("/api/reports/breakdown", post(reports::run_breakdown))
        .route("/api/reports/insights", post(reports::run_breakdown))
        .route("/api/reports/errors", post(reports::run_errors))
        .route("/api/reports/entry-exit", post(reports::run_entry_exit))
        .route("/api/reports/entry-pages", post(reports::run_entry_exit))
        .route("/api/reports/exit-pages", post(reports::run_entry_exit))
        .route("/api/reports/engagement", post(reports::run_engagement))
        .route("/api/reports/scroll", post(reports::run_engagement))
        .route("/api/boards", get(boards::list).post(boards::create))
        .route(
            "/api/boards/{id}",
            get(boards::get).post(boards::update).delete(boards::delete),
        )
        .route("/api/boards/{id}/clone", post(boards::clone))
        .route(
            "/api/boards/{id}/shares",
            get(share::get_entity_shares).post(share::create_board_share),
        )
        .route("/api/links", get(links::list).post(links::create))
        .route("/api/links/charts", get(websites::charts))
        .route(
            "/api/links/{id}",
            get(links::get).post(links::update).delete(links::delete),
        )
        .route(
            "/api/links/{id}/shares",
            get(share::get_entity_shares).post(share::create_link_share),
        )
        .route("/api/pixels", get(pixels::list).post(pixels::create))
        .route("/api/pixels/charts", get(websites::charts))
        .route(
            "/api/pixels/{id}",
            get(pixels::get).post(pixels::update).delete(pixels::delete),
        )
        .route(
            "/api/pixels/{id}/image",
            post(pixels::upload_image).delete(pixels::delete_image),
        )
        .route(
            "/api/pixels/{id}/shares",
            get(share::get_entity_shares).post(share::create_pixel_share),
        )
        .route("/api/share", get(share::list).post(share::create))
        .route(
            "/api/share/id/{id}",
            get(share::get_by_id)
                .post(share::update_by_id)
                .delete(share::delete_by_id),
        )
        .route("/api/share/{slug}", get(share::get_by_slug))
        .route("/api/realtime/{id}", get(realtime::data))
        .route("/api/realtime/{id}/stream", get(realtime::stream))
        .route("/api/websites/{id}/realtime/stream", get(realtime::stream))
        .route("/api/alerts", get(alerts::list).post(alerts::create))
        .route("/api/websites/{id}/alerts", get(alerts::list_for_website))
        .route(
            "/api/alerts/{id}",
            get(alerts::get).post(alerts::update).delete(alerts::delete),
        )
        .route("/api/alerts/{id}/test", post(alerts::test_alert))
        .route(
            "/api/websites/{id}/alerts/history",
            get(alerts::list_history),
        )
        .route(
            "/api/websites/{id}/annotations",
            get(annotations::list_for_website).post(annotations::create),
        )
        .route(
            "/api/annotations/{id}",
            get(annotations::get)
                .post(annotations::update)
                .delete(annotations::delete),
        )
        .route("/api/health", get(health))
        .route("/healthz", get(health))
        .route("/readyz", get(health))
        .route("/api/version", get(system_version))
        .route("/api/system/version", get(system_version));

    if let Some(tracker_names) = get_env("TRACKER_SCRIPT_NAME") {
        for name in tracker_names
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let path = format!("/{}", name.trim_start_matches('/'));
            if path != "/script.js" && path != "/umami.js" {
                router = router.route(&path, get(tracker_script));
            }
        }
    }

    if let Some(endpoint) = get_env("COLLECT_API_ENDPOINT") {
        let endpoint = endpoint.trim();
        let path = format!("/{}", endpoint.trim_start_matches('/'));
        if !endpoint.is_empty() {
            if path != "/api/send" {
                router = router.route(&path, post(ingest::send));
            }
        }
    }

    let max_age_secs = get_env("CORS_MAX_AGE")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(86400);

    let cors = if let Some(allowed) = get_env("ALLOWED_ORIGINS").or_else(|| get_env("CORS_ORIGIN"))
    {
        let origins: Vec<axum::http::HeaderValue> = allowed
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::PATCH,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers(tower_http::cors::AllowHeaders::mirror_request())
            .allow_credentials(true)
            .max_age(std::time::Duration::from_secs(max_age_secs))
    } else {
        CorsLayer::permissive().max_age(std::time::Duration::from_secs(max_age_secs))
    };

    let force_ssl =
        get_env("FORCE_SSL").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

    router
        .layer(axum::middleware::from_fn_with_state(
            SecurityHeadersConfig { force_ssl },
            security_headers_middleware,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn security_headers_middleware(
    axum::extract::State(config): axum::extract::State<SecurityHeadersConfig>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        "x-dns-prefetch-control",
        axum::http::HeaderValue::from_static("on"),
    );
    headers.insert(
        "x-content-type-options",
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "x-frame-options",
        axum::http::HeaderValue::from_static("SAMEORIGIN"),
    );
    if config.force_ssl {
        headers.insert(
            axum::http::header::STRICT_TRANSPORT_SECURITY,
            axum::http::HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"),
        );
    }
    response
}

async fn tracker_script() -> impl IntoResponse {
    let script = include_str!("../../../webui/public/script.js");
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, must-revalidate",
            ),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        script,
    )
}

async fn telemetry_script() -> impl IntoResponse {
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, must-revalidate",
            ),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        "console.log('telemetry disabled');",
    )
}

async fn recorder_script() -> impl IntoResponse {
    let script = include_str!("../../../webui/public/recorder.js");
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, must-revalidate",
            ),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        script,
    )
}

async fn health() -> &'static str {
    "ok"
}

async fn system_version() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "version": "3.3.0",
        "kombuVersion": env!("CARGO_PKG_VERSION")
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_route() {
        let app = Router::new()
            .route("/api/health", get(health))
            .route("/healthz", get(health))
            .route("/readyz", get(health))
            .route("/api/version", get(system_version))
            .route("/api/system/version", get(system_version));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let response_healthz = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response_healthz.status(), StatusCode::OK);

        let response_ver = app
            .oneshot(
                Request::builder()
                    .uri("/api/system/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response_ver.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_tracker_script_route() {
        let app = Router::new().route("/script.js", get(tracker_script));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/script.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .unwrap(),
            "application/javascript; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_recorder_script_route() {
        let app = Router::new().route("/recorder.js", get(recorder_script));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/recorder.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .unwrap(),
            "application/javascript; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_config_route() {
        let app = Router::new().route("/api/config", get(config::get));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_two_factor_status_route() {
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let state = AppState {
            website_cache: Arc::new(Cache::new(100)),
            session_cache: Arc::new(Cache::new(100)),
            ingest_queue: IngestQueue::new(&pool, 100, 1),
            app_secret: Arc::new("test-secret".into()),
            rate_limiter: crate::rate_limit::RateLimiter::new(),
            pool,
        };
        let app = Router::new()
            .route("/api/2fa/status", get(two_factor::status))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/2fa/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_report_funnel_route() {
        let app = Router::new().route("/api/reports/funnel", post(reports::run_funnel));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports/funnel")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"steps":["/","/signup"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_report_retention_route() {
        let app = Router::new().route("/api/reports/journey", post(reports::run_journey));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports/journey")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_report_revenue_route() {
        let app = Router::new().route("/api/reports/revenue", post(reports::run_revenue));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports/revenue")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_build_router_with_lookup_custom_env() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".into());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let app = build_router_with_lookup(pool.clone(), &|k| match k {
            "TRACKER_SCRIPT_NAME" => Some("custom-script.js, , /alt-script.js, /script.js".into()),
            "COLLECT_API_ENDPOINT" => Some("/custom-send".into()),
            "ALLOWED_ORIGINS" => Some("https://example.com,https://sub.example.com".into()),
            "CORS_MAX_AGE" => Some("3600".into()),
            "FORCE_SSL" => Some("true".into()),
            _ => None,
        });

        let res_script = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/custom-script.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_script.status(), StatusCode::OK);
        assert!(
            res_script
                .headers()
                .contains_key(axum::http::header::STRICT_TRANSPORT_SECURITY)
        );

        let res_collect = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/custom-send")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(res_collect.status(), StatusCode::NOT_FOUND);

        let app_send = build_router_with_lookup(pool.clone(), &|k| match k {
            "COLLECT_API_ENDPOINT" => Some("/api/send".into()),
            _ => None,
        });
        let res_default_send = app_send
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/send")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let app_empty = build_router_with_lookup(pool.clone(), &|k| match k {
            "COLLECT_API_ENDPOINT" => Some("   ".into()),
            _ => None,
        });
        let res_empty_send = app_empty
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/send")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(res_default_send.status(), StatusCode::NOT_FOUND);
        assert_ne!(res_empty_send.status(), StatusCode::NOT_FOUND);
    }
}
