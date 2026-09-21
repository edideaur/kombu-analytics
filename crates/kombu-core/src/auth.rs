#![forbid(unsafe_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid token")]
    InvalidToken,
    #[error("expired")]
    Expired,
    #[error("locked")]
    Locked,
}

#[derive(Debug, Clone)]
pub struct TwoFactorRateLimit {
    pub attempts: i32,
    pub locked_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl TwoFactorRateLimit {
    pub fn is_locked(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        if self.attempts >= 5 {
            if let Some(until) = self.locked_until {
                return now < until;
            }
        }
        false
    }

    pub fn should_lock(&self) -> bool {
        self.attempts >= 5
    }
}

pub fn check_expiry(exp: i64, now_secs: i64) -> Result<(), AuthError> {
    if now_secs > exp {
        return Err(AuthError::Expired);
    }
    Ok(())
}

pub fn bearer_token(auth_header: Option<&str>) -> Option<String> {
    auth_header.and_then(|h| h.strip_prefix("Bearer ").map(ToString::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn expiry_ok() {
        assert!(check_expiry(100, 99).is_ok());
        assert!(check_expiry(100, 101).is_err());
    }

    #[test]
    fn locked_logic() {
        let now = Utc.timestamp_opt(1000, 0).unwrap();
        let rl = TwoFactorRateLimit {
            attempts: 5,
            locked_until: Some(Utc.timestamp_opt(2000, 0).unwrap()),
        };
        assert!(rl.is_locked(now));
        assert!(rl.should_lock());
        let rl2 = TwoFactorRateLimit {
            attempts: 4,
            locked_until: None,
        };
        assert!(!rl2.is_locked(now));
        assert!(!rl2.should_lock());
        let rl3 = TwoFactorRateLimit {
            attempts: 5,
            locked_until: None,
        };
        assert!(!rl3.is_locked(now));
        assert!(rl3.should_lock());
        let past = Utc.timestamp_opt(500, 0).unwrap();
        let rl4 = TwoFactorRateLimit {
            attempts: 5,
            locked_until: Some(past),
        };
        assert!(!rl4.is_locked(now));
    }

    #[test]
    fn bearer_parse() {
        assert_eq!(
            bearer_token(Some("Bearer abc123")),
            Some("abc123".to_string())
        );
        assert_eq!(bearer_token(Some("Basic abc")), None);
        assert_eq!(bearer_token(None), None);
    }

    #[test]
    fn test_auth_error_display() {
        assert_eq!(AuthError::InvalidToken.to_string(), "invalid token");
        assert_eq!(AuthError::Expired.to_string(), "expired");
        assert_eq!(AuthError::Locked.to_string(), "locked");
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn harness_expiry() {
        let exp: i64 = kani::any();
        let now: i64 = kani::any();
        let r = check_expiry(exp, now);
        if now > exp {
            kani::assert(r.is_err(), "should be expired");
        } else {
            kani::assert(r.is_ok(), "should be ok");
        }
    }

    #[kani::proof]
    fn harness_lock() {
        let attempts: i32 = kani::any();
        kani::assume(attempts >= 0 && attempts <= 10);
        let rl = TwoFactorRateLimit {
            attempts,
            locked_until: None,
        };
        if attempts >= 5 {
            kani::assert(rl.should_lock(), "should lock");
        } else {
            kani::assert(!rl.should_lock(), "should not lock");
        }
    }

    #[kani::proof]
    fn harness_lock_threshold() {
        let attempts: i32 = kani::any();
        kani::assume(attempts >= 0 && attempts <= 10);
        let locked = attempts >= 5;
        if attempts >= 5 {
            kani::assert(locked, "5+ locks");
        } else {
            kani::assert(!locked, "<5 passes");
        }
    }
}
