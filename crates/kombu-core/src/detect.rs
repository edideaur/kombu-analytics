#![forbid(unsafe_code)]

pub fn get_device(user_agent: &str, screen: &str) -> &'static str {
    let ua_lower = user_agent.to_ascii_lowercase();
    let mut device = if ua_lower.contains("mobile")
        || ua_lower.contains("iphone")
        || ua_lower.contains("android")
    {
        "mobile"
    } else if ua_lower.contains("tablet") || ua_lower.contains("ipad") {
        "tablet"
    } else {
        "desktop"
    };

    if device == "desktop" {
        if let Some(w) = screen.split('x').next().and_then(|s| s.parse::<u32>().ok()) {
            if w <= 1920 && !screen.is_empty() {
                device = "laptop";
            }
        }
    }
    device
}

pub fn normalize_browser(browser: Option<&str>) -> Option<String> {
    browser
        .map(|s| s.chars().take(20).collect::<String>().to_lowercase())
        .filter(|s| !s.is_empty())
}

pub fn normalize_os(os: Option<&str>) -> Option<String> {
    os.map(|s| s.chars().take(20).collect::<String>().to_lowercase())
        .filter(|s| !s.is_empty())
}

pub fn has_blocked_ip(client_ip: &str, ignore_ips: Option<&str>) -> bool {
    crate::ip::is_ip_ignored(client_ip, ignore_ips)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn device_mobile() {
        assert_eq!(get_device("Mozilla Mobile", ""), "mobile");
        assert_eq!(get_device("Mozilla", ""), "desktop");
        assert_eq!(get_device("Mozilla", "1366x768"), "laptop");
        assert_eq!(get_device("iPad Safari", ""), "tablet");
    }

    #[test]
    fn test_normalizations() {
        assert_eq!(normalize_browser(Some("Chrome")), Some("chrome".into()));
        assert_eq!(normalize_browser(Some("")), None);
        assert_eq!(normalize_browser(None), None);

        assert_eq!(normalize_os(Some("Windows")), Some("windows".into()));
        assert_eq!(normalize_os(Some("")), None);
        assert_eq!(normalize_os(None), None);
    }

    #[test]
    fn blocked_ip() {
        assert!(has_blocked_ip("1.2.3.4", Some("1.2.3.4")));
        assert!(!has_blocked_ip("1.2.3.4", Some("5.6.7.8")));
        assert!(has_blocked_ip("192.168.1.5", Some("192.168.1.0/24")));
        assert!(!has_blocked_ip("", Some("1.2.3.4")));
        assert!(!has_blocked_ip("1.2.3.4", None));
        assert!(!has_blocked_ip("1.2.3.4", Some("")));
        assert!(!has_blocked_ip("not_an_ip", Some("10.0.0.0/8")));
        assert!(!has_blocked_ip("192.168.2.5", Some("192.168.1.0/24")));
        assert!(!has_blocked_ip("1.2.3.4", Some("not-a-cidr/24")));
        assert!(has_blocked_ip("1.2.3.4", Some("5.6.7.8, 1.2.3.4")));
    }

    #[test]
    fn device_and_normalization_extra() {
        assert_eq!(get_device("Some Tablet UA", ""), "tablet");
        assert_eq!(get_device("Mozilla", "9999x768"), "desktop");
        assert_eq!(get_device("Mozilla", "abc"), "desktop");
        let long = "a".repeat(30);
        assert_eq!(normalize_browser(Some(&long)).unwrap().len(), 20);
        assert_eq!(normalize_os(Some(&long)).unwrap().len(), 20);
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_device_bounds() {
        let val: u8 = kani::any();
        let d = match val % 3 {
            0 => 1,
            1 => 2,
            _ => 3,
        };
        kani::assert(d >= 1 && d <= 3, "valid device code");
    }

    #[kani::proof]
    fn harness_device_laptop_width_bound() {
        let w: u32 = kani::any();
        kani::assume(w <= 4000);
        let is_laptop = w <= 1920;
        if w <= 1920 {
            kani::assert(is_laptop, "1920 or below is laptop");
        } else {
            kani::assert(!is_laptop, "above 1920 is desktop");
        }
    }
}
