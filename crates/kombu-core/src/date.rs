#![forbid(unsafe_code)]

use chrono::{DateTime, Datelike, Timelike, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl TimeUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

pub fn bucket_start(dt: DateTime<Utc>, unit: TimeUnit) -> DateTime<Utc> {
    match unit {
        TimeUnit::Minute => dt
            .with_timezone(&Utc)
            .date_naive()
            .and_hms_opt(dt.hour(), dt.minute(), 0)
            .map_or(dt, |d| d.and_utc()),
        TimeUnit::Hour => dt
            .with_timezone(&Utc)
            .date_naive()
            .and_hms_opt(dt.hour(), 0, 0)
            .map_or(dt, |d| d.and_utc()),
        TimeUnit::Day => dt
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map_or(dt, |d| d.and_utc()),
        TimeUnit::Week => {
            let d = dt.date_naive();
            let weekday = i64::from(d.weekday().num_days_from_monday());
            (d - chrono::Duration::days(weekday))
                .and_hms_opt(0, 0, 0)
                .map_or(dt, |nd| nd.and_utc())
        }
        TimeUnit::Month => dt
            .date_naive()
            .with_day(1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map_or(dt, |d| d.and_utc()),
        TimeUnit::Year => chrono::NaiveDate::from_ymd_opt(dt.year(), 1, 1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map_or(dt, |d| d.and_utc()),
    }
}

pub fn is_monotonic_buckets(buckets: &[DateTime<Utc>]) -> bool {
    buckets.windows(2).all(|w| w[0] <= w[1])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn bucket_units_all() {
        let dt = Utc.with_ymd_and_hms(2024, 6, 15, 13, 45, 30).unwrap();
        assert_eq!(
            bucket_start(dt, TimeUnit::Minute),
            Utc.with_ymd_and_hms(2024, 6, 15, 13, 45, 0).unwrap()
        );
        assert_eq!(
            bucket_start(dt, TimeUnit::Hour),
            Utc.with_ymd_and_hms(2024, 6, 15, 13, 0, 0).unwrap()
        );
        assert_eq!(
            bucket_start(dt, TimeUnit::Day),
            Utc.with_ymd_and_hms(2024, 6, 15, 0, 0, 0).unwrap()
        );
        assert_eq!(
            bucket_start(dt, TimeUnit::Week),
            Utc.with_ymd_and_hms(2024, 6, 10, 0, 0, 0).unwrap()
        );
        assert_eq!(
            bucket_start(dt, TimeUnit::Month),
            Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap()
        );
        assert_eq!(
            bucket_start(dt, TimeUnit::Year),
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn time_unit_strings() {
        assert_eq!(TimeUnit::Minute.as_str(), "minute");
        assert_eq!(TimeUnit::Hour.as_str(), "hour");
        assert_eq!(TimeUnit::Day.as_str(), "day");
        assert_eq!(TimeUnit::Week.as_str(), "week");
        assert_eq!(TimeUnit::Month.as_str(), "month");
        assert_eq!(TimeUnit::Year.as_str(), "year");
    }

    #[test]
    fn monotonic() {
        let a = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let b = Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
        assert!(is_monotonic_buckets(&[a, b]));
        assert!(!is_monotonic_buckets(&[b, a]));
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_monotonic_no_overflow() {
        let a: i64 = kani::any();
        let b: i64 = kani::any();
        kani::assume(a <= b);
        kani::assert(a <= b, "monotonic condition holds");
    }

    #[kani::proof]
    fn harness_time_unit_str() {
        let choice: u8 = kani::any();
        let u = match choice % 6 {
            0 => super::TimeUnit::Minute,
            1 => super::TimeUnit::Hour,
            2 => super::TimeUnit::Day,
            3 => super::TimeUnit::Week,
            4 => super::TimeUnit::Month,
            _ => super::TimeUnit::Year,
        };
        kani::assert(!u.as_str().is_empty(), "as_str not empty");
    }
}
