# Migrating from Umami to Kombu

Kombu is designed as a **100% drop-in replacement** for Umami 3.3.0. It runs on the identical PostgreSQL schema, supports the same tracking scripts and HTTP endpoints, and accepts existing user passwords without requiring resets.

---

## Key Benefits of Migrating

* **Memory & CPU Efficiency**: Typical memory consumption drops from ~400 to 800 MB (Node.js runtime) down to ~96 MB baseline (safe Rust with `jemalloc`).
* **Higher Throughput**: Sustains >19,000 requests/second with zero dropped events using a lock-free 32-shard ring buffer.
* **Strict Memory Safety**: Built entirely with `#![forbid(unsafe_code)]`.
* **Zero Client Changes**: Existing `<script src=".../script.js" data-website-id="...">` tags on websites continue to work without any modification.
* **Enterprise Storage Options**: Seamlessly upgrade to PostgreSQL declarative range partitioning, TimescaleDB, or ClickHouse columnar storage when traffic grows.

---

## Compatibility Matrix

| Feature / Component | Umami 3.3.0 | Kombu Analytics | Migration Action |
|---|---|---|---|
| **Database Schema** | PostgreSQL 14+ | PostgreSQL 14+ | None (identical schema) |
| **Tracker Script** | `script.js` / `tracker.js` | `script.js` / `tracker.js` | None (drop-in compatible) |
| **Ingestion Endpoints** | `/api/send`, `/collect` | `/api/send`, `/collect` | None (drop-in compatible) |
| **REST APIs** | `/api/websites`, `/api/auth`, etc. | Umami 3.3.0 REST specification | None |
| **Authentication** | bcrypt password hashes | bcrypt & argon2 (auto-verifying) | None (existing passwords work) |
| **Two-Factor Auth** | TOTP | RFC 6238 TOTP + backup codes | Works out of the box |
| **User & Team RBAC** | Admin, User, Team roles | Full Umami RBAC parity | Preserved intact |
| **Reports & Dashboards** | Funnels, Insights, Retention | 100% parity + Revenue/Attribution | Preserved intact |

---

## Pre-Migration Checklist

1. **Backup Your Database**: Always take a database dump before switching backends:
   ```bash
   pg_dump -Fc -d umami -f umami_backup_$(date +%Y%m%d).dump
   ```
2. **Verify PostgreSQL Version**: Ensure PostgreSQL is version 14 or higher.
3. **Note Your Environment**: Collect your existing database credentials (`DATABASE_URL`) and application secret (`APP_SECRET`).

---

## Migration Method 1: In-Place Upgrade (Recommended, Fastest)

If your existing Umami instance already connects to PostgreSQL, you can point Kombu directly to that same database.

### Step 1: Stop the Umami Service

```bash
docker compose -f docker-compose.umami.yml down
```

Or systemd:
```bash
sudo systemctl stop umami
```

### Step 2: Run Kombu Migrations

Kombu includes migrations that add high-performance composite indexes and extended feature tables (such as custom image pixels, alert channels, annotations, and automated data retention) without dropping or altering existing Umami tables:

```bash
DATABASE_URL="postgres://umami:password@localhost:5432/umami" kombu migrate
```

### Step 3: Start Kombu

#### Option A: Docker Compose

Replace your Umami service in your `compose.yaml`:

```yaml
services:
  kombu:
    image: ghcr.io/kombu-analytics/kombu:latest
    container_name: kombu
    restart: always
    environment:
      DATABASE_URL: postgres://umami:password@postgres:5432/umami
      APP_SECRET: "your_existing_umami_app_secret_here"
      DATABASE_MAX_CONNECTIONS: 80
      COLLECT_RATE_LIMIT: 3000
    ports: ["3000:3000"]
    depends_on: ["postgres"]
```

#### Option B: Standalone Binary / Systemd

```bash
export DATABASE_URL="postgres://umami:password@localhost:5432/umami"
export APP_SECRET="your_existing_umami_app_secret_here"
kombu serve --listen 0.0.0.0:3000
```

---

## Migration Method 2: Side-by-Side Migration with a New Database

If you want to test Kombu in parallel with zero impact on production Umami:

1. **Clone the Umami database**:
   ```bash
   createdb -O umami kombu
   pg_dump -Fc -d umami | pg_restore -d kombu
   ```

2. **Run Kombu migrations on the new database**:
   ```bash
   DATABASE_URL="postgres://umami:password@localhost:5432/kombu" kombu migrate
   ```

3. **Start Kombu on an alternate port (e.g. 3001)**:
   ```bash
   DATABASE_URL="postgres://umami:password@localhost:5432/kombu" APP_SECRET="your_secret" kombu serve --listen 0.0.0.0:3001
   ```

4. **Verify through the web UI at `http://localhost:3001`**:
   * Log in with your existing Umami username and password.
   * Verify all websites, historical charts, custom reports, and teams appear accurately.

5. **Cut over your reverse proxy** (Caddy, Nginx, Cloudflare) from port 3000 to Kombu.

---

## Migration Method 3: JSON File Import via CLI

If you exported your Umami events/websites as a JSON file, you can import it directly into any Kombu website:

```bash
kombu import-umami \
  --website-id <WEBSITE_UUID> \
  --file /path/to/umami-export.json
```

Or via the HTTP import endpoint:
```bash
POST /api/websites/:websiteId/import
```

---

## Operational CLI Utilities

Kombu provides full operational tools to manage users, reset passwords, inspect system health, and export data:

```bash
# Check system diagnostics, latency, and migration status
kombu doctor

# Create an admin or standard user
kombu user create --username newadmin --password "StrongPassword123!" --role admin

# Reset a forgotten password without SQL surgery
kombu user reset-password --username admin --password "NewPassword123!"

# List all registered users
kombu user list

# Export events to CSV or JSON
kombu export --website-id <UUID> --format csv --output backup.csv
```

---

## Environment Variable Mapping

| Umami Variable | Kombu Equivalent | Description |
|---|---|---|
| `DATABASE_URL` | `DATABASE_URL` | PostgreSQL connection string |
| `APP_SECRET` | `APP_SECRET` | Secret used to salt session hashes and sign JWT tokens |
| `CLIENT_IP_HEADER` | `CLIENT_IP_HEADER` | Header to read visitor IP from (e.g. `cf-connecting-ip`, `x-forwarded-for`) |
| `REMOVE_TRAILING_SLASH` | Built-in | Path normalization is automatic in Kombu |
| `TRACKER_SCRIPT_NAME` | `TRACKER_SCRIPT_NAME` | Custom script path alias (defaults to `script.js`) |
| `COLLECT_API_ENDPOINT`| `COLLECT_API_ENDPOINT`| Custom collect endpoint alias (defaults to `/api/send`) |
| `DISABLE_BOT_CHECK` | `DISABLE_BOT_CHECK` | Set to `1` or `true` to disable bot detection |
| (none) | `DATABASE_MAX_CONNECTIONS` | Max PostgreSQL pool connections (default: `50`) |
| (none) | `COLLECT_RATE_LIMIT` | Ingestion burst limit per IP per minute (default: `3000`) |
| (none) | `STORAGE_ENGINE` | `postgres` (default), `partitioned`, `timescale`, or `clickhouse` |

---

## Client Tracker Verification

Websites already loading the tracker script need **zero changes**:

```html
<script
  defer
  src="https://analytics.example.com/script.js"
  data-website-id="3e2d6bf8-5f21-4f18-9ce2-1596e1b69a23"
></script>
```

Custom event tracking calls also remain identical:

```javascript
umami.track('signup-button-clicked', { plan: 'pro' });
```

---

## Scaling Past 50 Million Events

Once migrated to Kombu, you can optionally enable partitioned rollups or alternative storage engines without touching client code:

```bash
kombu migrate --engine partitioned
```
Set in environment: `STORAGE_ENGINE=partitioned`
