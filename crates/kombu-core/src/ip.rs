#![forbid(unsafe_code)]

use std::net::IpAddr;

pub const IP_ADDRESS_HEADERS: [&str; 12] = [
    "true-client-ip",
    "cf-connecting-ip",
    "fastly-client-ip",
    "x-nf-client-connection-ip",
    "do-connecting-ip",
    "x-real-ip",
    "x-appengine-user-ip",
    "x-forwarded-for",
    "forwarded",
    "x-client-ip",
    "x-cluster-client-ip",
    "x-forwarded",
];

pub fn strip_port(ip: &str) -> &str {
    let trimmed = ip.trim();
    if trimmed.is_empty() {
        return trimmed;
    }

    if let Some(without_prefix) = trimmed.strip_prefix('[') {
        if let Some((host, _)) = without_prefix.split_once(']') {
            return host;
        }
    }

    if let Some(idx) = trimmed.rfind(':') {
        let prefix = &trimmed[..idx];
        if prefix.contains('.') || !prefix.contains(':') {
            return prefix;
        }
    }

    trimmed
}

pub fn normalize_ip(ip: &str) -> String {
    let stripped = strip_port(ip);
    if let Ok(parsed) = stripped.parse::<IpAddr>() {
        match parsed {
            IpAddr::V6(v6) => {
                if let Some(v4) = v6.to_ipv4_mapped() {
                    return v4.to_string();
                }
                v6.to_string()
            }
            IpAddr::V4(v4) => v4.to_string(),
        }
    } else {
        stripped.to_string()
    }
}

pub fn parse_header_ip(header_name: &str, header_val: &str) -> Option<String> {
    let lower_name = header_name.to_ascii_lowercase();
    let clean_val = header_val.trim();

    if clean_val.is_empty() {
        return None;
    }

    if lower_name == "x-forwarded-for" {
        let first = clean_val.split(',').next().unwrap_or(clean_val).trim();
        return Some(normalize_ip(first));
    }

    if lower_name == "forwarded" {
        for part in clean_val.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("for=") {
                let ip_str = rest.trim_matches('"').trim();
                return Some(normalize_ip(ip_str));
            }
        }
    }

    Some(normalize_ip(clean_val))
}

pub fn resolve_client_ip<'a, I>(headers: I, custom_header: Option<&str>) -> Option<String>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let headers_vec: Vec<(&'a str, &'a str)> = headers.into_iter().collect();

    if let Some(custom) = custom_header.filter(|s| !s.is_empty()) {
        let custom_lower = custom.to_ascii_lowercase();
        for (k, v) in &headers_vec {
            if k.eq_ignore_ascii_case(&custom_lower) {
                if let Some(resolved) = parse_header_ip(k, v) {
                    return Some(resolved);
                }
            }
        }
    }

    for target in IP_ADDRESS_HEADERS {
        for (k, v) in &headers_vec {
            if k.eq_ignore_ascii_case(target) {
                if let Some(resolved) = parse_header_ip(k, v) {
                    return Some(resolved);
                }
            }
        }
    }

    None
}

pub fn is_ip_ignored(client_ip: &str, ignore_ips: Option<&str>) -> bool {
    let ignore = match ignore_ips {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => std::env::var("IGNORE_IP").unwrap_or_default(),
    };

    if client_ip.is_empty() || ignore.is_empty() {
        return false;
    }

    let client_ip_clean = normalize_ip(client_ip);

    for token in ignore.split(',').map(str::trim) {
        if token.is_empty() {
            continue;
        }
        if token == client_ip || token == client_ip_clean {
            return true;
        }
        if token.contains('/') {
            if let (Ok(net), Ok(addr)) = (
                token.parse::<ipnet::IpNet>(),
                client_ip_clean.parse::<IpAddr>(),
            ) {
                if net.contains(&addr) {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_port() {
        assert_eq!(strip_port("1.2.3.4:8080"), "1.2.3.4");
        assert_eq!(strip_port("1.2.3.4"), "1.2.3.4");
        let s = format!("[{}]:8080", "2001:db8::1");
        assert_eq!(strip_port(&s), "2001:db8::1");
        let s2 = format!("[{}]:8080", "::1");
        assert_eq!(strip_port(&s2), "::1");
        assert_eq!(strip_port("[invalid-bracket"), "[invalid-bracket");
        assert_eq!(strip_port("2001:db8::1"), "2001:db8::1");
        assert_eq!(strip_port("   "), "");
    }

    #[test]
    fn test_normalize_ip() {
        assert_eq!(normalize_ip("192.168.1.100"), "192.168.1.100");
        assert_eq!(normalize_ip("192.168.1.100:3000"), "192.168.1.100");
        assert_eq!(normalize_ip("::ffff:192.0.2.128"), "192.0.2.128");
        assert_eq!(normalize_ip("2001:db8::1"), "2001:db8::1");
        assert_eq!(normalize_ip("invalid-ip"), "invalid-ip");
    }

    #[test]
    fn test_parse_header_ip() {
        assert_eq!(
            parse_header_ip("x-forwarded-for", "1.1.1.1, 2.2.2.2"),
            Some("1.1.1.1".into())
        );
        assert_eq!(
            parse_header_ip(
                "forwarded",
                "proto=https;for=\"198.51.100.17\";by=203.0.113.60"
            ),
            Some("198.51.100.17".into())
        );
        assert_eq!(
            parse_header_ip("cf-connecting-ip", "1.2.3.4"),
            Some("1.2.3.4".into())
        );
        assert_eq!(parse_header_ip("any", "   "), None);
    }

    #[test]
    fn test_resolve_client_ip() {
        let headers = [
            ("user-agent", "Mozilla"),
            ("x-forwarded-for", "9.9.9.9, 8.8.8.8"),
            ("cf-connecting-ip", "1.1.1.1"),
        ];

        let ip = resolve_client_ip(headers.to_vec(), None);
        assert_eq!(ip, Some("1.1.1.1".into()));

        let headers_custom = [
            ("my-custom-ip", "   "),
            ("other-header", "1.2.3.4"),
            ("my-custom-ip", "10.20.30.40"),
            ("cf-connecting-ip", "1.1.1.1"),
        ];
        let ip_custom = resolve_client_ip(headers_custom.to_vec(), Some("my-custom-ip"));
        assert_eq!(ip_custom, Some("10.20.30.40".into()));

        let headers_standard_fallback = [("cf-connecting-ip", "   "), ("x-real-ip", "5.6.7.8")];
        assert_eq!(
            resolve_client_ip(headers_standard_fallback.to_vec(), None),
            Some("5.6.7.8".into())
        );

        let empty_headers: [(&str, &str); 1] = [("accept", "*/*")];
        assert_eq!(resolve_client_ip(empty_headers.to_vec(), None), None);
    }

    #[test]
    fn test_is_ip_ignored() {
        assert!(is_ip_ignored("10.0.0.1", Some("10.0.0.0/8, 192.168.1.1")));
        assert!(is_ip_ignored(
            "192.168.1.1",
            Some(", , 10.0.0.0/8, 192.168.1.1, , ")
        ));
        assert!(!is_ip_ignored(
            "192.168.1.2",
            Some("10.0.0.0/8, 192.168.1.1")
        ));
        assert!(!is_ip_ignored("8.8.8.8", Some("")));
        assert!(!is_ip_ignored("", Some("10.0.0.1")));
    }
}
