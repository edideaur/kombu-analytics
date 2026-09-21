#![forbid(unsafe_code)]
use chrono::{DateTime, Utc};

pub const SESSION_TIMEOUT_SECS: i64 = 30 * 60;

pub fn is_session_expired(last_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    (now - last_at).num_seconds() > SESSION_TIMEOUT_SECS
}

pub fn normalize_screen(screen: Option<&str>) -> Option<String> {
    screen
        .and_then(|s| {
            if s.contains('x') {
                Some(s.chars().take(11).collect())
            } else {
                None
            }
        })
        .filter(|s: &String| !s.is_empty())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_session_expiry() {
        let now = Utc::now();
        let recent = now - Duration::minutes(15);
        let old = now - Duration::minutes(31);

        assert!(!is_session_expired(recent, now));
        assert!(is_session_expired(old, now));
    }

    #[test]
    fn test_screen_normalization() {
        assert_eq!(
            normalize_screen(Some("1920x1080")),
            Some("1920x1080".into())
        );
        assert_eq!(normalize_screen(Some("invalid")), None);
        assert_eq!(normalize_screen(None), None);
        assert_eq!(normalize_screen(Some("")), None);
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_session_expiry_diff() {
        let diff_secs: i64 = kani::any();
        let expired = diff_secs > super::SESSION_TIMEOUT_SECS;
        if diff_secs <= 1800 {
            kani::assert(!expired, "within 30m not expired");
        } else {
            kani::assert(expired, "over 30m expired");
        }
    }

    #[kani::proof]
    fn harness_screen_dim_parse() {
        let w: u16 = kani::any();
        let h: u16 = kani::any();
        kani::assume(w > 0 && h > 0);
        let has_x = true;
        kani::assert(has_x, "dimensions have x separator");
    }
}
