#![forbid(unsafe_code)]

use axum::{
    Json,
    http::{HeaderMap, StatusCode},
};
use quick_cache::sync::Cache;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub struct RateLimiter {
    cache: Arc<Cache<String, (u32, i64)>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Cache::new(500_000)),
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn check(
        &self,
        key: &str,
        max_requests: u32,
        window_seconds: i64,
    ) -> Result<(), (StatusCode, HeaderMap, Json<Value>)> {
        if max_requests == 0 {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp();
        if let Some((count, start)) = self.cache.get(key) {
            if now - start >= window_seconds {
                self.cache.insert(key.to_string(), (1, now));
                Ok(())
            } else if count < max_requests {
                self.cache.insert(key.to_string(), (count + 1, start));
                Ok(())
            } else {
                let retry_after = (window_seconds - (now - start)).max(1);
                let mut headers = HeaderMap::new();
                let val = axum::http::HeaderValue::from(retry_after);
                headers.insert("retry-after", val);
                Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    headers,
                    Json(json!({
                        "error": "Too many requests, please try again later",
                        "retryAfter": retry_after
                    })),
                ))
            }
        } else {
            self.cache.insert(key.to_string(), (1, now));
            Ok(())
        }
    }
}

#[must_use]
pub fn extract_ip(headers: &HeaderMap) -> String {
    let custom_header = std::env::var("CLIENT_IP_HEADER").ok();
    let iter = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.as_str(), s)));
    kombu_core::ip::resolve_client_ip(iter, custom_header.as_deref())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_unit() {
        let limiter = RateLimiter::default();
        let key = "test_key";
        assert!(limiter.check(key, 0, 60).is_ok());

        assert!(limiter.check(key, 2, 60).is_ok());
        assert!(limiter.check(key, 2, 60).is_ok());
        let res_err = limiter.check(key, 2, 60);
        assert!(res_err.is_err());
        let (status, headers, body) = res_err.unwrap_err();
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert!(headers.contains_key("retry-after"));
        assert!(body.0.get("retryAfter").is_some());

        assert!(limiter.check(key, 2, 0).is_ok());

        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.1".parse().unwrap());
        assert_eq!(extract_ip(&headers), "198.51.100.1");

        let empty_headers = HeaderMap::new();
        assert_eq!(extract_ip(&empty_headers), "127.0.0.1");
    }
}
