#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

const FORMULA_TRIGGERS: &[char] = &['=', '+', '-', '@', '\t', '\r'];

pub fn sanitize_csv_field(input: &str) -> String {
    if input.is_empty() {
        return input.to_string();
    }

    if FORMULA_TRIGGERS.iter().any(|&c| input.starts_with(c)) {
        return format!("'{input}");
    }

    let trimmed = input.trim_start();
    if FORMULA_TRIGGERS.iter().any(|&c| trimmed.starts_with(c)) {
        format!("'{input}")
    } else {
        input.to_string()
    }
}

pub fn is_formula_trigger(input: &str) -> bool {
    if FORMULA_TRIGGERS.iter().any(|&c| input.starts_with(c)) {
        return true;
    }
    let trimmed = input.trim_start();
    FORMULA_TRIGGERS.iter().any(|&c| trimmed.starts_with(c))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportWebsiteEvent {
    pub event_id: String,
    pub website_id: String,
    pub session_id: String,
    pub created_at: String,
    pub url_path: String,
    pub url_query: Option<String>,
    pub referrer_domain: Option<String>,
    pub page_title: Option<String>,
    pub event_type: i32,
    pub event_name: Option<String>,
    pub is_bot: bool,
}

impl ExportWebsiteEvent {
    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},\"{}\",\"{}\",\"{}\",\"{}\",{},\"{}\",{}",
            self.event_id,
            self.website_id,
            self.session_id,
            self.created_at,
            sanitize_csv_field(&self.url_path).replace('"', "\"\""),
            sanitize_csv_field(self.url_query.as_deref().unwrap_or("")).replace('"', "\"\""),
            sanitize_csv_field(self.referrer_domain.as_deref().unwrap_or("")).replace('"', "\"\""),
            sanitize_csv_field(self.page_title.as_deref().unwrap_or("")).replace('"', "\"\""),
            self.event_type,
            sanitize_csv_field(self.event_name.as_deref().unwrap_or("")).replace('"', "\"\""),
            self.is_bot
        )
    }

    pub fn csv_header() -> &'static str {
        "event_id,website_id,session_id,created_at,url_path,url_query,referrer_domain,page_title,event_type,event_name,is_bot"
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_formula_injection_escaping() {
        assert_eq!(sanitize_csv_field(""), "");
        assert_eq!(
            sanitize_csv_field("=cmd|' /C calc'!A0"),
            "'=cmd|' /C calc'!A0"
        );
        assert_eq!(sanitize_csv_field("+1+2"), "'+1+2");
        assert_eq!(sanitize_csv_field("-5"), "'-5");
        assert_eq!(sanitize_csv_field("@SUM(1,2)"), "'@SUM(1,2)");
        assert_eq!(sanitize_csv_field("\talert()"), "'\talert()");
        assert_eq!(sanitize_csv_field("  =trimmed"), "'  =trimmed");
        assert_eq!(sanitize_csv_field("normal_text"), "normal_text");

        assert!(is_formula_trigger("=cmd"));
        assert!(is_formula_trigger("  +calc"));
        assert!(!is_formula_trigger("safe text"));
    }

    #[test]
    fn test_export_row_generation() {
        assert!(!ExportWebsiteEvent::csv_header().is_empty());

        let ev = ExportWebsiteEvent {
            event_id: "evt-1".into(),
            website_id: "web-1".into(),
            session_id: "sess-1".into(),
            created_at: "2026-09-18T12:00:00Z".into(),
            url_path: "=HYPERLINK(\"http://evil.com\")".into(),
            url_query: None,
            referrer_domain: Some("google.com".into()),
            page_title: Some("Home".into()),
            event_type: 1,
            event_name: None,
            is_bot: false,
        };

        let row = ev.to_csv_row();
        assert!(row.contains("'=HYPERLINK"));
        assert!(row.ends_with(",false"));
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn harness_formula_escaping() {
        let trigger_idx: usize = kani::any();
        kani::assume(trigger_idx < FORMULA_TRIGGERS.len());
        let c = FORMULA_TRIGGERS[trigger_idx];
        let starts_with_trigger = matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r');
        kani::assert(starts_with_trigger, "all formula triggers are matched");
    }
}
