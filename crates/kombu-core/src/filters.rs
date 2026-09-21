#![forbid(unsafe_code)]

use crate::constants::FILTER_COLUMN_MAP;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BitmaskFilter {
    pub mask: u32,
}

impl BitmaskFilter {
    pub const MASK_PAGEVIEW: u32 = 1 << 0;
    pub const MASK_CUSTOM_EVENT: u32 = 1 << 1;
    pub const MASK_LINK_CLICK: u32 = 1 << 2;
    pub const MASK_PIXEL: u32 = 1 << 3;
    pub const MASK_PERFORMANCE: u32 = 1 << 4;
    pub const MASK_ERROR: u32 = 1 << 5;
    pub const MASK_BOT: u32 = 1 << 6;
    pub const MASK_DESKTOP: u32 = 1 << 7;
    pub const MASK_MOBILE: u32 = 1 << 8;
    pub const MASK_TABLET: u32 = 1 << 9;

    pub const fn empty() -> Self {
        Self { mask: 0 }
    }

    pub const fn all() -> Self {
        Self { mask: u32::MAX }
    }

    #[must_use]
    pub fn with_flag(mut self, flag: u32) -> Self {
        self.mask |= flag;
        self
    }

    #[inline]
    pub fn matches(&self, event_mask: u32) -> bool {
        (self.mask & event_mask) != 0
    }

    #[inline]
    pub const fn event_type_to_mask(event_type: i32) -> u32 {
        match event_type {
            1 => Self::MASK_PAGEVIEW,
            2 => Self::MASK_CUSTOM_EVENT,
            3 => Self::MASK_LINK_CLICK,
            4 => Self::MASK_PIXEL,
            5 => Self::MASK_PERFORMANCE,
            6 => Self::MASK_ERROR,
            _ => 0,
        }
    }
}

pub fn filter_column(name: &str) -> Option<&'static str> {
    FILTER_COLUMN_MAP
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
}

pub fn build_where_clause(filters: &[(String, String)]) -> String {
    let mut clauses = Vec::new();
    for (idx, (k, _)) in filters.iter().enumerate() {
        if let Some(col) = filter_column(k) {
            clauses.push(format!("{col} = ${}", idx + 1));
        }
    }
    if clauses.is_empty() {
        "1=1".to_string()
    } else {
        clauses.join(" AND ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist() {
        assert_eq!(filter_column("browser"), Some("browser"));
        assert_eq!(filter_column("path"), Some("url_path"));
        assert_eq!(filter_column("injection; DROP"), None);
    }

    #[test]
    fn test_bitmask_filter() {
        let empty = BitmaskFilter::empty();
        assert_eq!(empty.mask, 0);
        assert!(!empty.matches(BitmaskFilter::MASK_PAGEVIEW));

        let all = BitmaskFilter::all();
        assert_eq!(all.mask, u32::MAX);
        assert!(all.matches(BitmaskFilter::MASK_PAGEVIEW));

        let custom = BitmaskFilter::default()
            .with_flag(BitmaskFilter::MASK_PAGEVIEW)
            .with_flag(BitmaskFilter::MASK_ERROR);
        assert!(custom.matches(BitmaskFilter::MASK_PAGEVIEW));
        assert!(custom.matches(BitmaskFilter::MASK_ERROR));
        assert!(!custom.matches(BitmaskFilter::MASK_LINK_CLICK));

        assert_eq!(
            BitmaskFilter::event_type_to_mask(1),
            BitmaskFilter::MASK_PAGEVIEW
        );
        assert_eq!(
            BitmaskFilter::event_type_to_mask(2),
            BitmaskFilter::MASK_CUSTOM_EVENT
        );
        assert_eq!(
            BitmaskFilter::event_type_to_mask(3),
            BitmaskFilter::MASK_LINK_CLICK
        );
        assert_eq!(
            BitmaskFilter::event_type_to_mask(4),
            BitmaskFilter::MASK_PIXEL
        );
        assert_eq!(
            BitmaskFilter::event_type_to_mask(5),
            BitmaskFilter::MASK_PERFORMANCE
        );
        assert_eq!(
            BitmaskFilter::event_type_to_mask(6),
            BitmaskFilter::MASK_ERROR
        );
        assert_eq!(BitmaskFilter::event_type_to_mask(999), 0);
    }

    #[test]
    fn where_clause_safe() {
        assert_eq!(build_where_clause(&[]), "1=1");
        let f = vec![
            ("browser".to_string(), "chrome".to_string()),
            ("country".to_string(), "US".to_string()),
            ("bad; DROP".to_string(), "x".to_string()),
        ];
        let sql = build_where_clause(&f);
        assert!(sql.contains("browser = $1"));
        assert!(sql.contains("country = $2"));
        assert!(!sql.contains("DROP"));
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn harness_allowlist_static() {
        for &(k, v) in FILTER_COLUMN_MAP {
            kani::assert(!k.contains(';'), "no injection in key");
            kani::assert(!v.contains(';'), "no injection in val");
            kani::assert(!v.contains(' '), "no spaces in val");
        }
    }
}
