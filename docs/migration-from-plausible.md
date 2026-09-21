# Migrating from Plausible to Kombu (CSV Export Guide)

Kombu includes first-class support for importing historical analytics from **Plausible Analytics** CSV exports. You can seamlessly preserve your visitor history, top pages, referrer sources, custom goals, and geographic analytics when transitioning to Kombu.

---

## Why Migrate from Plausible to Kombu?

* **Umami 3.3.0 Ecosystem & Rich Analytics**: Gain funnels, cohort retention analysis, custom multi-metric boards, session activity timelines, and revenue attribution out of the box.
* **100% Safe Rust Core**: `#![forbid(unsafe_code)]` runtime with ultra-low resource usage (~96 MB baseline RAM).
* **No Ingestion Limits or Penalties**: Scale past tens of millions of monthly pageviews without SaaS tier caps.
* **Flexible Backend Scaling**: Choose between PostgreSQL, declarative range-partitioned rollups, TimescaleDB, or ClickHouse columnar storage.

---

## Step 1: Export Data from Plausible

### On Plausible Cloud or Self-Hosted Plausible:

1. Log into your Plausible instance and open your site's dashboard.
2. Navigate to **Site Settings** (gear icon in the top right).
3. In the left navigation, select **Data Management** (or scroll to the **Danger Zone** in older releases).
4. Click **Export Data** (or **Download stats as CSV**).
5. Plausible will prepare a `.zip` file and prompt you to download it.
6. Extract the downloaded zip archive. You will see CSV files containing historical stats:
   * `visitors.csv` / `imported_visitors.csv`
   * `custom_events.csv`
   * or a unified `events.csv` / `plausible_export.csv`

Kombu's importer accepts both unified event exports and individual pageview/event CSV files.

---

## Step 2: Plausible CSV Header & Field Compatibility

Kombu's parser automatically detects and normalizes standard Plausible CSV column naming conventions:

| Plausible Column Name Variants | Mapped Field in Kombu | Notes |
|---|---|---|
| `date`, `time` | `created_at` | Parses ISO 8601 / RFC 3339 timestamps (defaults to `00:00:00Z` if time is omitted) |
| `page`, `page_path`, `url_path`, `path`, `url` | `url_path` | Normalized with leading `/` and sanitized against CSV formula injection |
| `entry_page`, `entry` | `entry_url` | Recorded with session entry context |
| `exit_page`, `exit` | `exit_url` | Recorded with session exit context |
| `bounce`, `bounced` | Session bounce flag | Accepts `true`, `1`, `yes` |
| `visit_duration`, `duration` | Session duration | Duration in seconds |
| `referrer`, `referrer_domain` | `referrer_domain` | Stripped and sanitized referrer host |
| `source`, `utm_source` | `utm_source` | Attribution campaign tracking |
| `country` | `country` | 2-letter ISO 3166-1 alpha-2 country code |
| `region` | `region` | ISO 3166-2 region or state code |
| `city` | `city` | Geolocation city name |
| `device` | `device` | Normalized device category (`desktop`, `mobile`, `tablet`) |
| `browser` | `browser` | Detected client browser name |
| `os`, `operating_system` | `os` | Detected operating system |
| `event_name`, `name`, `goal` | `event_name` / `event_type` | Imported as custom events (`event_type: 2`); standard pageviews default to `event_type: 1` |

### Security & Sanitization
The importer strictly guards against CSV formula injection (`=`, `+`, `-`, `@`, `\t`, `\r`) by stripping executable prefix characters, and enforces strict relational integrity by generating corresponding session records for each unique visitor visit.

---

## Step 3: Create the Website in Kombu

Before running the import, you must have a target website in Kombu:

1. Log into your Kombu dashboard (`http://localhost:3000`).
2. Go to **Settings** -> **Websites** -> **Add Website**.
3. Enter your domain (e.g. `example.com`) and website name.
4. Copy the generated **Website ID** (a UUID such as `9d545957-8842-5a13-bee7-8b9eb9ce6f51`).

---

## Step 4: Import Using the Kombu CLI

The fastest and most reliable way to import historical CSV data is using the `kombu import-plausible` command:

```bash
export DATABASE_URL="postgres://kombu:kombu@localhost:5432/kombu"
kombu import-plausible \
  --website-id 9d545957-8842-5a13-bee7-8b9eb9ce6f51 \
  --file /path/to/plausible_export.csv
```

### CLI Execution Output:
```text
Successfully imported 48,290 events from Plausible CSV into website 9d545957-8842-5a13-bee7-8b9eb9ce6f51
```

### Ingestion Details:
* Imports run in transactions chunked at **1,000 rows per batch** for minimal lock contention and high PostgreSQL throughput.
* If a chunk fails, the transaction safely rolls back with a detailed error message without corrupting existing database records.

---

## Step 5: (Alternative) Import via REST API

If you are running Kombu in a containerized or managed environment without direct shell access, you can use the HTTP Import endpoint:

```bash
curl -X POST "http://localhost:3000/api/websites/9d545957-8842-5a13-bee7-8b9eb9ce6f51/import" \
  -H "Authorization: Bearer <YOUR_ADMIN_JWT_TOKEN>" \
  -H "Content-Type: application/json" \
  -d '[
    {
      "urlPath": "/blog/hello-world",
      "referrerDomain": "news.ycombinator.com",
      "country": "US",
      "device": "desktop",
      "browser": "Chrome",
      "os": "Linux",
      "createdAt": "2026-03-01T14:30:00Z"
    }
  ]'
```

---

## Step 6: Pre-Compute Hourly Rollups (Optional but Recommended)

After importing thousands or millions of historical records, generate pre-computed hourly stats to ensure immediate sub-second dashboard loading across all historical date ranges:

```bash
kombu rollup --website-id 9d545957-8842-5a13-bee7-8b9eb9ce6f51
```

---

## Step 7: Update Tracking Script on Your Websites

Replace the Plausible tracking script in your HTML templates with Kombu's lightweight tracking tag:

### Old (Plausible):
```html
<script defer data-domain="example.com" src="https://plausible.io/js/script.js"></script>
```

### New (Kombu):
```html
<script
  defer
  src="https://analytics.example.com/script.js"
  data-website-id="9d545957-8842-5a13-bee7-8b9eb9ce6f51"
></script>
```

### Custom Event & Goal Tracking:
If you use Plausible's JavaScript function for goal tracking:

```javascript
plausible('Download', { props: { version: '2.0' } });
```

Kombu and Umami equivalent:
```javascript
umami.track('Download', { version: '2.0' });
```

---

## Verification

1. Open your Kombu dashboard.
2. Select your newly imported website.
3. Change the date picker range to match your historical Plausible data (e.g. "Last 12 Months" or "All Time").
4. Confirm that total pageviews, unique visitors, top pages, referrer sources, and countries match your Plausible records.
