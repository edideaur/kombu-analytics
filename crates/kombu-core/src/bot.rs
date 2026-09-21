#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BotClassification {
    pub is_bot: bool,
    pub score: u8,
    pub reason: Option<&'static str>,
}

pub static KNOWN_SPAM_REFERRERS: &[&str] = &[
    "darodar.com",
    "semalt.com",
    "buttons-for-website.com",
    "ilovevitaly.com",
    "priceg.com",
    "blackhatworth.com",
    "buy-cheap-traffic.com",
    "traffic2cash.xyz",
    "floating-share-buttons.com",
    "free-share-buttons.top",
    "site-auditor.online",
    "web-revenue.xyz",
    "event-tracking.com",
    "trafficmonetize.org",
    "4webmasters.org",
    "best-seo-solution.com",
    "best-seo-offer.com",
    "free-floating-buttons.com",
    "get-free-traffic-now.com",
    "torture.ml",
    "savetubevideo.com",
    "aliexpress.com.free-traffic.org",
    "hulfingtonpost.com",
    "makemoneyonline.com",
    "o-o-8-o-o.com",
    "humanorbot.com",
    "buttons-for-your-website.com",
    "success-seo.com",
    "videos-for-your-site.com",
    "share-buttons.xyz",
];

const BOT_PATTERNS: &[&str] = &[
    "bot",
    "spider",
    "crawl",
    "slurp",
    "mediapartners",
    "headless",
    "phantom",
    "selenium",
    "puppeteer",
    "playwright",
    "cypress",
    "curl/",
    "wget/",
    "python-requests",
    "aiohttp",
    "httpclient",
    "postman",
    "ahrefs",
    "semrush",
    "feedfetcher",
    "facebookexternalhit",
    "bytespider",
    "yandexbot",
    "duckduckgo",
    "bingbot",
    "googlebot",
    "applebot",
    "inspect",
    "screaming frog",
    "uptimerobot",
    "pingdom",
];

pub fn is_spam_referrer(domain: &str) -> bool {
    let without_port = domain.trim().split(':').next().unwrap_or(domain);
    let without_www = without_port.strip_prefix("www.").unwrap_or(without_port);
    let clean = without_www.to_ascii_lowercase();

    if clean.is_empty() {
        return false;
    }

    for &spam in KNOWN_SPAM_REFERRERS {
        if clean == spam || clean.ends_with(&format!(".{spam}")) {
            return true;
        }
    }

    false
}

static ISBOT_DETECTOR: std::sync::LazyLock<isbot::Bots> =
    std::sync::LazyLock::new(isbot::Bots::default);

pub fn is_bot_user_agent(ua: &str) -> (bool, u8, Option<&'static str>) {
    let trimmed = ua.trim();
    if trimmed.is_empty() {
        return (true, 80, Some("empty_user_agent"));
    }
    if trimmed.len() < 5 {
        return (true, 70, Some("suspicious_short_ua"));
    }

    if ISBOT_DETECTOR.is_bot(trimmed) {
        return (true, 95, Some("isbot_matched"));
    }

    let ua_lower = trimmed.to_ascii_lowercase();
    for &pat in BOT_PATTERNS {
        if ua_lower.contains(pat) {
            return (true, 90, Some("pattern_matched"));
        }
    }

    (false, 0, None)
}

pub fn classify_bot(
    ua: Option<&str>,
    screen: Option<&str>,
    referrer_domain: Option<&str>,
    webdriver: bool,
) -> BotClassification {
    let mut score = 0u8;
    let mut reason: Option<&'static str> = None;

    if webdriver {
        return BotClassification {
            is_bot: true,
            score: 100,
            reason: Some("webdriver_detected"),
        };
    }

    if let Some(ref_domain) = referrer_domain {
        if is_spam_referrer(ref_domain) {
            score = score.saturating_add(80);
            reason = Some("referrer_spam");
        }
    }

    if let Some(user_agent) = ua {
        let (is_ua_bot, ua_score, ua_reason) = is_bot_user_agent(user_agent);
        if is_ua_bot {
            score = score.max(ua_score);
            if reason.is_none() {
                reason = ua_reason;
            }
        }
    } else {
        score = score.saturating_add(60);
        if reason.is_none() {
            reason = Some("missing_user_agent");
        }
    }

    if let Some(s) = screen {
        if s.trim() == "0x0" {
            score = score.saturating_add(40);
            if reason.is_none() {
                reason = Some("zero_resolution");
            }
        }
    }
    BotClassification {
        is_bot: score >= 50,
        score,
        reason,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_bot_detection_user_agents() {
        let (bot, _, reason) = is_bot_user_agent("");
        assert!(bot);
        assert_eq!(reason, Some("empty_user_agent"));

        let (bot, _, reason) = is_bot_user_agent("abc");
        assert!(bot);
        assert_eq!(reason, Some("suspicious_short_ua"));

        let (bot, _, _) = is_bot_user_agent("Googlebot/2.1 (+http://www.google.com/bot.html)");
        assert!(bot);

        let (bot, _, _) =
            is_bot_user_agent("Mozilla/5.0 (compatible; AhrefsBot/7.0; +http://ahrefs.com/robot/)");
        assert!(bot);

        let (bot, _, _) = is_bot_user_agent("HeadlessChrome/120.0.0.0");
        assert!(bot);

        let (bot, _, _) = is_bot_user_agent("curl/7.68.0");
        assert!(bot);

        let (bot, _, _) =
            is_bot_user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36");
        assert!(!bot);
    }

    #[test]
    fn test_spam_referrer() {
        assert!(is_spam_referrer("darodar.com"));
        assert!(is_spam_referrer("subdomain.semalt.com"));
        assert!(is_spam_referrer("buttons-for-website.com:8080"));
        assert!(!is_spam_referrer(""));
        assert!(!is_spam_referrer("github.com"));
        assert!(!is_spam_referrer("google.com"));
    }

    #[test]
    fn test_classify_bot() {
        let res_webdriver = classify_bot(None, None, None, true);
        assert!(res_webdriver.is_bot);
        assert_eq!(res_webdriver.score, 100);
        assert_eq!(res_webdriver.reason, Some("webdriver_detected"));

        let res_existing_reason_zero_screen = classify_bot(None, Some("0x0"), None, false);
        assert!(res_existing_reason_zero_screen.is_bot);
        assert_eq!(
            res_existing_reason_zero_screen.reason,
            Some("missing_user_agent")
        );

        let res_spam_with_ua_bot =
            classify_bot(Some("Googlebot/2.1"), None, Some("darodar.com"), false);
        assert!(res_spam_with_ua_bot.is_bot);
        assert_eq!(res_spam_with_ua_bot.reason, Some("referrer_spam"));

        let res_spam_with_none_ua = classify_bot(None, None, Some("darodar.com"), false);
        assert!(res_spam_with_none_ua.is_bot);
        assert_eq!(res_spam_with_none_ua.reason, Some("referrer_spam"));

        let res_spam_with_zero_screen = classify_bot(
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
            Some("0x0"),
            Some("darodar.com"),
            false,
        );
        assert!(res_spam_with_zero_screen.is_bot);
        assert_eq!(res_spam_with_zero_screen.reason, Some("referrer_spam"));

        let (bot, score, reason) = is_bot_user_agent("custom-test-agent/puppeteer-internal");
        assert!(bot);
        assert_eq!(score, 90);
        assert_eq!(reason, Some("pattern_matched"));

        let res_screen_fallback = classify_bot(
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
            Some("1920x1080"),
            None,
            false,
        );
        assert!(!res_screen_fallback.is_bot);

        let res_screen_none = classify_bot(
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
            None,
            None,
            false,
        );
        assert!(!res_screen_none.is_bot);

        let res_zero_screen = classify_bot(
            Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"),
            Some("0x0"),
            None,
            false,
        );
        assert_eq!(res_zero_screen.reason, Some("zero_resolution"));

        let res_zero_screen_prior = classify_bot(Some("curl/7.88.1"), Some("0x0"), None, false);
        assert!(res_zero_screen_prior.is_bot);

        let res = classify_bot(Some("curl/7.88.1"), Some("1920x1080"), None, false);
        assert!(res.is_bot);
        assert!(res.reason.is_some());

        let res_legit = classify_bot(
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0 Safari/537.36"),
            Some("1920x1080"),
            Some("google.com"),
            false,
        );
        assert!(!res_legit.is_bot);

        let res_spam = classify_bot(
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0 Safari/537.36"),
            Some("1920x1080"),
            Some("semalt.com"),
            false,
        );
        assert!(res_spam.is_bot);
        assert_eq!(res_spam.reason, Some("referrer_spam"));
    }
}
