#![forbid(unsafe_code)]

pub const EVENT_NAME_MAX: usize = 50;
pub const FIELD_LENGTH_URL_PATH: usize = 500;
pub const FIELD_LENGTH_STRING_VALUE: usize = 500;
pub const FIELD_LENGTH_DATA_KEY: usize = 500;
pub const EVENT_DATA_MAX_KEYS: usize = 50;
pub const DATA_TYPE_STRING: i32 = 1;
pub const DATA_TYPE_NUMBER: i32 = 2;
pub const DATA_TYPE_BOOLEAN: i32 = 3;
pub const DATA_TYPE_DATE: i32 = 4;
pub const DATA_TYPE_ARRAY: i32 = 5;

pub const EVENT_TYPE_PAGE_VIEW: i32 = 1;
pub const EVENT_TYPE_CUSTOM_EVENT: i32 = 2;
pub const EVENT_TYPE_LINK_EVENT: i32 = 3;
pub const EVENT_TYPE_PIXEL_EVENT: i32 = 4;
pub const EVENT_TYPE_PERFORMANCE: i32 = 5;
pub const EVENT_TYPE_ERROR: i32 = 6;

pub const DEFAULT_LOCALE: &str = "en-US";
pub const DEFAULT_THEME: &str = "light";
pub const DEFAULT_PAGE_SIZE: usize = 20;
pub const MAX_PAGING_RESULTS: usize = 10000;
pub const REALTIME_RANGE_SECS: i64 = 30;
pub const REALTIME_INTERVAL_MS: u64 = 10000;

pub const EVENT_COLUMNS: &[&str] = &[
    "path",
    "fullPath",
    "entry",
    "exit",
    "referrer",
    "domain",
    "title",
    "query",
    "event",
    "tag",
    "hostname",
    "utmSource",
    "utmMedium",
    "utmCampaign",
    "utmContent",
    "utmTerm",
];

pub const SESSION_COLUMNS: &[&str] = &[
    "browser",
    "os",
    "device",
    "screen",
    "language",
    "country",
    "city",
    "region",
    "distinctId",
];

pub const FILTER_COLUMN_MAP: &[(&str, &str)] = &[
    ("path", "url_path"),
    ("entry", "url_path"),
    ("exit", "url_path"),
    ("referrer", "referrer_domain"),
    ("domain", "referrer_domain"),
    ("hostname", "hostname"),
    ("distinctId", "distinct_id"),
    ("title", "page_title"),
    ("query", "url_query"),
    ("os", "os"),
    ("browser", "browser"),
    ("device", "device"),
    ("country", "country"),
    ("region", "region"),
    ("city", "city"),
    ("language", "language"),
    ("event", "event_name"),
    ("tag", "tag"),
    ("eventType", "event_type"),
    ("utmSource", "utm_source"),
    ("utmMedium", "utm_medium"),
    ("utmCampaign", "utm_campaign"),
    ("utmContent", "utm_content"),
    ("utmTerm", "utm_term"),
];

pub const SHARE_TOKEN_HEADER: &str = "x-umami-share-token";
pub const SHARE_CONTEXT_HEADER: &str = "x-umami-share-context";
pub const CACHE_TOKEN_TYPE: &str = "cache";
pub const SHARE_TOKEN_TYPE: &str = "share";

pub const DATETIME_REGEX: &str = r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}";
