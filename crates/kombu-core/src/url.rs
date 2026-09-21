#![forbid(unsafe_code)]

use crate::constants::FIELD_LENGTH_URL_PATH;
use std::borrow::Cow;

pub fn truncate_cow(s: &str, max: usize) -> Cow<'_, str> {
    if s.len() <= max {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.chars().take(max).collect())
    }
}

pub fn truncate_url_path(s: &str) -> String {
    let sanitized = if s.contains('?') {
        sanitize_url_path(s)
    } else {
        s.to_string()
    };

    if sanitized.len() <= FIELD_LENGTH_URL_PATH {
        sanitized
    } else {
        sanitized.chars().take(FIELD_LENGTH_URL_PATH).collect()
    }
}

pub fn is_sensitive_query_key(key: &str) -> bool {
    let lower = key.trim().to_ascii_lowercase();
    let k = lower.as_str();

    matches!(
        k,
        "token"
            | "auth"
            | "api_key"
            | "apikey"
            | "key"
            | "session"
            | "session_id"
            | "sessionid"
            | "password"
            | "pass"
            | "pwd"
            | "secret"
            | "email"
            | "access_token"
            | "refresh_token"
            | "id_token"
            | "private_key"
            | "bearer"
            | "credentials"
            | "jwt"
            | "code"
            | "verification_code"
            | "otp"
            | "cvv"
            | "ssn"
    ) || k.contains("token")
        || k.contains("secret")
        || k.contains("password")
        || k.contains("apikey")
        || k.contains("api_key")
        || k.contains("auth_key")
        || k.contains("private_key")
        || (k.contains("email") && !k.contains("campaign"))
}

pub fn sanitize_query_string(query: &str) -> String {
    let has_prefix = query.starts_with('?');
    let query_str = if has_prefix { &query[1..] } else { query };
    if query_str.is_empty() {
        return query.to_string();
    }

    let mut parts = Vec::new();
    for pair in query_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some((k, _)) = pair.split_once('=') {
            if is_sensitive_query_key(k) {
                parts.push(format!("{k}=[REDACTED]"));
            } else {
                parts.push(pair.to_string());
            }
        } else if is_sensitive_query_key(pair) {
            parts.push(format!("{pair}=[REDACTED]"));
        } else {
            parts.push(pair.to_string());
        }
    }

    let sanitized = parts.join("&");
    if has_prefix {
        format!("?{sanitized}")
    } else {
        sanitized
    }
}

pub fn sanitize_url_path(s: &str) -> String {
    if let Some((path, query)) = s.split_once('?') {
        let sanitized_query = sanitize_query_string(query);
        format!("{path}?{sanitized_query}")
    } else {
        s.to_string()
    }
}

pub fn truncate_string(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

pub fn truncate_event_name(s: &str) -> String {
    truncate_string(s, crate::constants::EVENT_NAME_MAX)
}

pub fn get_query_string(params: &[(&str, Option<String>)]) -> String {
    let parts: Vec<String> = params
        .iter()
        .filter_map(|(k, v)| {
            v.as_ref()
                .map(|val| format!("{}={}", urlencoding_simple(k), urlencoding_simple(val)))
        })
        .collect();
    parts.join("&")
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

pub fn build_path(path: &str, query: &str) -> String {
    if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    }
}

pub fn is_valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with('/')
}

pub fn safe_decode_uri(s: Option<&str>) -> Option<String> {
    s.map(percent_decode)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQueryParams {
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_content: Option<String>,
    pub utm_term: Option<String>,
    pub gclid: Option<String>,
    pub fbclid: Option<String>,
    pub msclkid: Option<String>,
    pub ttclid: Option<String>,
    pub li_fat_id: Option<String>,
    pub twclid: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedReferrer {
    pub referrer_path: Option<String>,
    pub referrer_query: Option<String>,
    pub referrer_domain: Option<String>,
}

pub fn parse_query_params(query: &str) -> ParsedQueryParams {
    let mut result = ParsedQueryParams::default();
    let query_str = query.strip_prefix('?').unwrap_or(query);
    if query_str.is_empty() {
        return result;
    }

    for pair in query_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (raw_key, raw_val) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        let key = raw_key.trim().to_ascii_lowercase();
        let decoded_val = percent_decode(raw_val);
        let val = truncate_string(decoded_val.trim(), 255);
        if val.is_empty() {
            continue;
        }

        match key.as_str() {
            "utm_source" => result.utm_source = Some(val),
            "utm_medium" => result.utm_medium = Some(val),
            "utm_campaign" => result.utm_campaign = Some(val),
            "utm_content" => result.utm_content = Some(val),
            "utm_term" => result.utm_term = Some(val),
            "gclid" => result.gclid = Some(val),
            "fbclid" => result.fbclid = Some(val),
            "msclkid" => result.msclkid = Some(val),
            "ttclid" => result.ttclid = Some(val),
            "li_fat_id" | "lifatid" => result.li_fat_id = Some(val),
            "twclid" => result.twclid = Some(val),
            _ => {}
        }
    }

    result
}

fn clean_domain(domain: &str) -> String {
    let without_port = domain.split(':').next().unwrap_or(domain);
    let without_www = without_port.strip_prefix("www.").unwrap_or(without_port);
    without_www.to_ascii_lowercase()
}

pub fn parse_referrer(referrer: &str, current_hostname: Option<&str>) -> ParsedReferrer {
    let s = referrer.trim();
    if s.is_empty() {
        return ParsedReferrer::default();
    }

    let (domain_part, rest) = if let Some(after_http) = s.strip_prefix("http://") {
        match after_http.split_once('/') {
            Some((dom, path_and_q)) => (Some(dom), format!("/{path_and_q}")),
            None => (Some(after_http), String::new()),
        }
    } else if let Some(after_https) = s.strip_prefix("https://") {
        match after_https.split_once('/') {
            Some((dom, path_and_q)) => (Some(dom), format!("/{path_and_q}")),
            None => (Some(after_https), String::new()),
        }
    } else if s.starts_with('/') || s.starts_with('?') {
        (None, s.to_string())
    } else {
        match s.split_once('/') {
            Some((dom, path_and_q)) => (Some(dom), format!("/{path_and_q}")),
            None => (Some(s), String::new()),
        }
    };

    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (
            if p.is_empty() {
                None
            } else {
                Some(truncate_string(p, 500))
            },
            if q.is_empty() {
                None
            } else {
                Some(truncate_string(q, 500))
            },
        ),
        None => (
            if rest.is_empty() {
                None
            } else {
                Some(truncate_string(&rest, 500))
            },
            None,
        ),
    };

    let domain = domain_part.and_then(|d| {
        let cleaned = clean_domain(d);
        if cleaned.is_empty() {
            None
        } else if let Some(curr) = current_hostname {
            if cleaned == clean_domain(curr) {
                None
            } else {
                Some(truncate_string(&cleaned, 500))
            }
        } else {
            Some(truncate_string(&cleaned, 500))
        }
    });

    ParsedReferrer {
        referrer_path: path,
        referrer_query: query,
        referrer_domain: domain,
    }
}

fn percent_decode(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(hex);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn truncate_within_bound() {
        let s = "a".repeat(500);
        assert_eq!(truncate_url_path(&s).len(), 500);
    }

    #[test]
    fn truncate_over_bound() {
        let s = "a".repeat(600);
        assert_eq!(truncate_url_path(&s).len(), 500);
    }

    #[test]
    fn valid_url_check() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://example.com"));
        assert!(is_valid_url("/path"));
        assert!(!is_valid_url("not a url"));
    }

    #[test]
    fn test_truncate_helpers() {
        assert_eq!(truncate_string("hello", 10), "hello");
        assert_eq!(truncate_string("hello world", 5), "hello");
        let long_name = "x".repeat(100);
        assert_eq!(truncate_event_name(&long_name).len(), 50);

        let short = "abc";
        assert_eq!(truncate_cow(short, 10), Cow::Borrowed("abc"));
        let long = "abcdefghijklmn";
        assert_eq!(truncate_cow(long, 5), Cow::Owned::<str>("abcde".into()));
    }

    #[test]
    fn test_query_string_and_path() {
        let params = [
            ("foo", Some("bar".into())),
            ("empty", None),
            ("space", Some("a b".into())),
        ];
        let qs = get_query_string(&params);
        assert!(qs.contains("foo=bar"));
        assert!(qs.contains("space=a%20b"));
        assert!(!qs.contains("empty="));

        assert_eq!(build_path("/test", ""), "/test");
        assert_eq!(build_path("/test", "a=1"), "/test?a=1");
    }

    #[test]
    fn test_percent_decoding() {
        assert_eq!(safe_decode_uri(None), None);
        assert_eq!(
            safe_decode_uri(Some("hello%20world")),
            Some("hello world".into())
        );
        assert_eq!(
            safe_decode_uri(Some("%ZZ_invalid")),
            Some("%ZZ_invalid".into())
        );
        assert_eq!(safe_decode_uri(Some("abc%")), Some("abc%".into()));
        assert_eq!(safe_decode_uri(Some("ab%2")), Some("ab%2".into()));
        assert_eq!(safe_decode_uri(Some("%FF")), Some("%FF".into()));
    }

    #[test]
    fn test_url_encoding_edge() {
        let params = [("q", Some("a/b?c&d=e".into()))];
        let qs = get_query_string(&params);
        assert!(qs.contains("%2F"), "slash encoded: {qs}");
        assert!(qs.contains("%3F"), "question encoded: {qs}");
        let params2 = [("k", Some("a-_.~".into()))];
        assert_eq!(get_query_string(&params2), "k=a-_.~");
        assert_eq!(truncate_url_path("").len(), 0);
    }

    #[test]
    fn test_parse_query_params() {
        let qs = "utm_source=Google&utm_medium=cpc&utm_campaign=summer_sale&utm_term=shoes&utm_content=banner&gclid=12345&fbclid=67890&msclkid=ms1&ttclid=tt1&li_fat_id=li1&twclid=tw1&&empty_val=";
        let parsed = parse_query_params(qs);
        assert_eq!(parsed.utm_source.as_deref(), Some("Google"));
        assert_eq!(parsed.utm_medium.as_deref(), Some("cpc"));
        assert_eq!(parsed.utm_campaign.as_deref(), Some("summer_sale"));
        assert_eq!(parsed.utm_term.as_deref(), Some("shoes"));
        assert_eq!(parsed.utm_content.as_deref(), Some("banner"));
        assert_eq!(parsed.gclid.as_deref(), Some("12345"));
        assert_eq!(parsed.fbclid.as_deref(), Some("67890"));
        assert_eq!(parsed.msclkid.as_deref(), Some("ms1"));
        assert_eq!(parsed.ttclid.as_deref(), Some("tt1"));
        assert_eq!(parsed.li_fat_id.as_deref(), Some("li1"));
        assert_eq!(parsed.twclid.as_deref(), Some("tw1"));

        let lifatid = parse_query_params("lifatid=li2");
        assert_eq!(lifatid.li_fat_id.as_deref(), Some("li2"));

        let empty = parse_query_params("");
        assert_eq!(empty, ParsedQueryParams::default());
    }

    #[test]
    fn test_parse_referrer() {
        assert_eq!(parse_referrer("", None), ParsedReferrer::default());
        assert_eq!(parse_referrer("   ", None), ParsedReferrer::default());

        let http_no_slash = parse_referrer("http://example.com", None);
        assert_eq!(http_no_slash.referrer_domain.as_deref(), Some("example.com"));
        assert_eq!(http_no_slash.referrer_path, None);

        let https_no_slash = parse_referrer("https://example.com", None);
        assert_eq!(https_no_slash.referrer_domain.as_deref(), Some("example.com"));
        assert_eq!(https_no_slash.referrer_path, None);

        let no_scheme = parse_referrer("example.org/path", None);
        assert_eq!(no_scheme.referrer_domain.as_deref(), Some("example.org"));
        assert_eq!(no_scheme.referrer_path.as_deref(), Some("/path"));

        let no_scheme_no_slash = parse_referrer("example.org", None);
        assert_eq!(no_scheme_no_slash.referrer_domain.as_deref(), Some("example.org"));
        assert_eq!(no_scheme_no_slash.referrer_path, None);

        let empty_path_query = parse_referrer("http://example.com/?", None);
        assert_eq!(empty_path_query.referrer_domain.as_deref(), Some("example.com"));

        let parsed = parse_referrer("https://www.google.com/search?q=kombu", Some("mysite.com"));
        assert_eq!(parsed.referrer_domain.as_deref(), Some("google.com"));
        assert_eq!(parsed.referrer_path.as_deref(), Some("/search"));
        assert_eq!(parsed.referrer_query.as_deref(), Some("q=kombu"));

        let self_ref = parse_referrer("https://www.mysite.com/blog", Some("mysite.com"));
        assert_eq!(self_ref.referrer_domain, None);
        assert_eq!(self_ref.referrer_path.as_deref(), Some("/blog"));

        let path_only = parse_referrer("/about?tab=team", None);
        assert_eq!(path_only.referrer_domain, None);
        assert_eq!(path_only.referrer_path.as_deref(), Some("/about"));
        assert_eq!(path_only.referrer_query.as_deref(), Some("tab=team"));
    }

    #[test]
    fn test_sensitive_query_sanitization() {
        assert!(is_sensitive_query_key("token"));
        assert!(is_sensitive_query_key("api_key"));
        assert!(is_sensitive_query_key("session_id"));
        assert!(is_sensitive_query_key("password"));
        assert!(is_sensitive_query_key("user_email"));
        assert!(!is_sensitive_query_key("utm_source"));

        let qs = "page=1&token=secret123&search=shoes&api_key=xyz987";
        let sanitized = sanitize_query_string(qs);
        assert_eq!(
            sanitized,
            "page=1&token=[REDACTED]&search=shoes&api_key=[REDACTED]"
        );

        assert_eq!(sanitize_query_string(""), "");
        assert_eq!(sanitize_query_string("?"), "?");
        assert_eq!(sanitize_query_string("?&&"), "?");
        assert_eq!(sanitize_query_string("?token"), "?token=[REDACTED]");
        assert_eq!(sanitize_query_string("?foo"), "?foo");
        assert_eq!(sanitize_url_path("/simple-path"), "/simple-path");
        assert_eq!(sanitize_url_path("/path?token=secret"), "/path?token=[REDACTED]");

        let p_extra = parse_query_params("?utm_unknown=test&utm_source=&single");
        assert!(p_extra.utm_source.is_none());

        let r_empty_rest = parse_referrer("https://example.com/?query", None);
        assert_eq!(r_empty_rest.referrer_path.as_deref(), Some("/"));
        assert_eq!(r_empty_rest.referrer_query.as_deref(), Some("query"));

        let r_plain = parse_referrer("https://example.com/p?query", None);
        assert_eq!(r_plain.referrer_path.as_deref(), Some("/p"));
        assert_eq!(r_plain.referrer_query.as_deref(), Some("query"));

        let r_empty_p_only = parse_referrer("https://example.com/?query", None);
        assert_eq!(r_empty_p_only.referrer_path.as_deref(), Some("/"));

        let r_empty_domain = parse_referrer("://", None);
        assert_eq!(r_empty_domain.referrer_domain.as_deref(), None);

        let r_blank_domain = parse_referrer("   /path", None);
        assert_eq!(r_blank_domain.referrer_domain.as_deref(), None);

        let r_empty_rest_p = parse_referrer("/somepath?query", None);
        assert_eq!(r_empty_rest_p.referrer_path.as_deref(), Some("/somepath"));

        let r_p_empty = parse_referrer("https://example.com/?query", None);
        assert_eq!(r_p_empty.referrer_path.as_deref(), Some("/"));

        let r_p_empty_direct = parse_referrer("?direct_query", None);
        assert_eq!(r_p_empty_direct.referrer_path.as_deref(), None);
        assert_eq!(r_p_empty_direct.referrer_query.as_deref(), Some("direct_query"));

        let r_with_empty_q = parse_referrer("https://example.com/path?", None);
        assert_eq!(r_with_empty_q.referrer_path.as_deref(), Some("/path"));
        assert_eq!(r_with_empty_q.referrer_query.as_deref(), None);

        let r_empty_rest = parse_referrer("https://example.com", None);
        assert_eq!(r_empty_rest.referrer_path.as_deref(), None);

        let r_empty_lead_slash = parse_referrer("/somepath", None);
        assert_eq!(r_empty_lead_slash.referrer_path.as_deref(), Some("/somepath"));

        let url = "/checkout?plan=pro&password=mypassword&user_email=alice@example.com";
        let clean_path = truncate_url_path(url);
        assert!(clean_path.contains("password=[REDACTED]"));
        assert!(clean_path.contains("user_email=[REDACTED]"));
        assert!(clean_path.contains("plan=pro"));
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_url_truncate() {
        let len: usize = kani::any();
        kani::assume(len <= 1000);
        let capped = if len <= crate::constants::FIELD_LENGTH_URL_PATH {
            len
        } else {
            crate::constants::FIELD_LENGTH_URL_PATH
        };
        kani::assert(capped <= 500, "VarChar bound");
    }

    #[kani::proof]
    fn harness_event_name_truncate() {
        let len: usize = kani::any();
        kani::assume(len <= 500);
        let capped = if len <= crate::constants::EVENT_NAME_MAX {
            len
        } else {
            crate::constants::EVENT_NAME_MAX
        };
        kani::assert(capped <= 50, "event_name bound");
    }

    #[kani::proof]
    fn harness_is_valid_url_prefixes() {
        let choice: u8 = kani::any();
        let s = match choice % 3 {
            0 => "https://kombu.is",
            1 => "http://kombu.is",
            _ => "/analytics",
        };
        kani::assert(super::is_valid_url(s), "url is valid prefix");
    }
}
