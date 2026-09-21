# Kombu Analytics

**Kombu** is a high-throughput, memory-safe, 100% Rust drop-in replacement backend and optimized SPA web UI for Umami 3.3.0.

Engineered with `#![forbid(unsafe_code)]` across all workspace crates and verified for rock-solid stability, zero Clippy warnings, and extreme concurrency handling (>19,000 requests/second).

---

## Key Highlights

* **100% Safe Rust:** `#![forbid(unsafe_code)]` enforced across all crates, unit tests, and integration test suites.
* **High Ingestion Throughput:** Sustains over **19,000 req/sec** with 0 errors, sub-second database flushes via 32-shard lock-free ring channels, and non-blocking asynchronous dispatch.
* **Umami 3.3.0 API & Schema Parity:** Full functional and data model parity covering analytics, retention, funnels, reports, event data, custom dashboards, teams, alerts, segments, user management, and two-factor authentication (TOTP).
* **Minimal Resource Footprint:** Consumes ~96 MB RAM at baseline and under 0.6 CPU cores (~57% CPU) at 20,000 req/sec with fixed OS thread pools.
* **Comprehensive Test & Code Coverage:** 100% line, function, and branch/region coverage across WebUI shims and 99.8%+ coverage across the entire Rust backend codebase.
* **Zero Clippy Warnings:** Fully compliant under strict `--pedantic` compiler lints (`cargo clippy --workspace --all-targets -- -D warnings`).

---

## Workspace Architecture

```
kombu-analytics/
├── crates/
│   ├── kombu-core/
│   ├── kombu-db/
│   ├── kombu-ingest/
│   ├── kombu-query/
│   ├── kombu-api/
│   └── kombu-cli/
├── webui/
├── docker/
└── Cargo.toml
```

---

## Benchmarks & Resource Profiling

Kombu was benchmarked against live PostgreSQL instances using multi-worker load pacing and real-time OS process counters:

| Target Throughput | CPU Usage (% of 1 Core) | Resident Memory (RSS) | System RAM % (of 32 GB) | Native OS Threads (`NLWP`) |
|---|---|---|---|---|
| **10 req/sec** | **1.2%** | **96.0 MB** | 0.30% | 15 |
| **100 req/sec** | **2.3%** | **100.2 MB** | 0.30% | 15 |
| **1,000 req/sec** | **7.2%** | **130.4 MB** | 0.40% | 15 |
| **5,000 req/sec** | **17.7%** | **432.7 MB** | 1.36% | 16 |
| **10,000 req/sec** | **33.7%** | **684.9 MB** | 2.12% | 15 |
| **20,000 req/sec** | **57.5%** | **788.6 MB** | 2.50% | 15 |

*Hotpath sustained stress test: **19,066.45 requests/second** sustained over 60 seconds with **1,163,721 requests processed and 0 errors (100.00% success rate)**.*

---

## Installation & Deployment

Complete operational instructions covering all environments are detailed in the **[Deployment Guide](docs/deployment.md)**.

### Quick Install (Linux, BSD, macOS)

Install the latest standalone precompiled binary with embedded Web UI:

```bash
curl -fsSL https://raw.githubusercontent.com/edideaur/kombu-analytics/main/scripts/install.sh | sudo sh
```

### Precompiled Standalone Binaries (GitHub Releases)

Single static binaries with zero external dependencies are published on every commit across 31 targets:

* **Linux x86_64:** `kombu-x86_64-unknown-linux-musl`, `kombu-x86_64-unknown-linux-gnu`
* **Linux ARM64:** `kombu-aarch64-unknown-linux-musl`, `kombu-aarch64-unknown-linux-gnu`
* **Linux ARMv7 / ARMv6 / ARMv5:** `armv7-musleabihf`, `armv7-gnueabihf`, `arm-musleabi`, `armv5te-musleabi`
* **Linux RISC-V / PowerPC / s390x:** `riscv64gc-gnu`, `powerpc-gnu`, `powerpc64-gnu`, `powerpc64le-gnu`, `s390x-gnu`
* **BSDs & Unix:** `x86_64-unknown-freebsd`, `i686-unknown-freebsd`, `x86_64-unknown-illumos`
* **macOS:** `universal-apple-darwin` (Universal lipo), `x86_64-apple-darwin`, `aarch64-apple-darwin`
* **Windows:** `x86_64-pc-windows-msvc.exe`, `x86_64-pc-windows-gnu.exe`, `aarch64-pc-windows-msvc.exe`

### Container Deployment (Docker & GHCR)

Pull the pre-built multi-architecture image (`linux/amd64`, `linux/arm64`):

```bash
docker pull ghcr.io/edideaur/kombu-analytics:latest
```

Launch with Docker Compose:

```bash
docker compose -f docker/compose.yaml up -d
```

### macOS and Linux with Colima

```bash
colima start kombu --config colima/colima.yaml
./colima/start-colima.sh
```

### Rootless Podman and Systemd

```bash
podman-compose -f podman/podman-compose.yml up -d
./podman/install-systemd-user-service
systemctl --user enable --now kombu.service
```

### Init System Services

Pre-configured service definitions and health check watchdogs are ready under `init/`:

* **systemd:** `init/systemd/kombu.service` (Ubuntu, Debian, RHEL, Arch, Fedora)
* **OpenRC:** `init/openrc/kombu.initd` (Alpine Linux, Gentoo, postmarketOS)
* **runit:** `init/runit/run` (Void Linux, Artix)
* **s6:** `init/s6/run` (Alpine s6, container appliances)
* **SysVinit:** `init/sysvinit/kombu` (Debian legacy, Devuan)
* **launchd:** `init/launchd/com.kombu.analytics.plist` (macOS native daemon)
* **FreeBSD rc.d:** `init/freebsd/kombu` (FreeBSD rc supervision)
* **Watchdogs:** `init/monit/kombu.monitrc`, `init/supervisord/kombu.conf`

See **[Init System Setup](init/README.md)** for step-by-step service installation.

---

## Building from Source

### Prerequisites

* **Rust 1.85+** (MSRV)
* **Node.js 20+** & **pnpm 9+**
* **C Compiler & Make:** gcc or clang and make (for jemalloc allocator)
* **PostgreSQL 14+**

### 1. Build the Web UI

```bash
cd webui
pnpm install --legacy-peer-deps
pnpm build
cd ..
```

### 2. Build the Standalone Binary

```bash
cargo build --release --bin kombu
```

The executable at `target/release/kombu` embeds the complete Vite SPA into binary memory.

### 3. Configure Environment Variables

```bash
export DATABASE_URL="postgres://kombu:kombu@localhost:5432/kombu"
export APP_SECRET="your_secure_32_character_random_app_secret_here"
export DATABASE_MAX_CONNECTIONS=80
export COLLECT_RATE_LIMIT=3000
```

### 4. Run Database Migrations

```bash
./target/release/kombu migrate
```

### 5. Start Kombu Server

```bash
./target/release/kombu serve --listen 0.0.0.0:3000
```

### 6. Import Historical Data from Plausible (Optional)

You can directly import Plausible Analytics CSV exports into any Kombu website:

```bash
./target/release/kombu import-plausible \
  --website-id <WEBSITE_UUID> \
  --file /path/to/plausible-export.csv
```

Alternatively, post the JSON/CSV array directly to the HTTP import endpoint:
```bash
POST /api/websites/:websiteId/import
```

---

## High-Scale Storage Engine Options

Kombu provides flexible analytics storage backends to fit every workload from small personal sites to multi-billion-event enterprise deployments:

| Storage Engine | Configuration | Best For | Architecture |
|---|---|---|---|
| **PostgreSQL (Default)** | `STORAGE_ENGINE=postgres` | Up to 50M events / year | Standard relational tables with composite B-tree indexes & async batch ingest. Single DB simplicity. |
| **PostgreSQL Partitioned + Rollups** | `STORAGE_ENGINE=partitioned` | 50M to 500M events / year | PostgreSQL Declarative Range Partitioning by `created_at` with pre-computed `website_event_stats_hourly` rollup aggregates. Zero extra infrastructure. |
| **TimescaleDB** | `STORAGE_ENGINE=timescale` or `TIMESCALE_ENABLED=1` | 100M to 1B+ events / year | Native TimescaleDB hypertables, automated 7-day chunking, and continuous aggregates with analytical compression. |
| **ClickHouse** | `STORAGE_ENGINE=clickhouse` or `CLICKHOUSE_URL=http://localhost:8123` | 1B to 10B+ events / year | Hybrid architecture: Relational metadata (users, teams, websites, reports) in PostgreSQL; raw high-velocity events in ClickHouse columnar storage (`ReplacingMergeTree`, vectorized SIMD aggregation). |

### CLI Engine Management

Run migrations for any selected engine:
```bash
cargo run --release -p kombu-cli -- migrate --engine partitioned
cargo run --release -p kombu-cli -- migrate --engine timescale
cargo run --release -p kombu-cli -- migrate --engine clickhouse --clickhouse-url http://localhost:8123
```

Pre-compute or refresh hourly rollups on demand:
```bash
cargo run --release -p kombu-cli -- rollup --website-id <WEBSITE_UUID>
```

---

## Testing & Code Quality

### Rust Verification

Run the full workspace test suite:
```bash
cargo test --workspace
```

Run strict Clippy pedantic checks:
```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Measure backend test coverage using `cargo-llvm-cov`:
```bash
cargo llvm-cov --workspace --summary-only
```

### Web UI Verification

Run frontend Vitest test suite with V8 code coverage:
```bash
cd webui
pnpm test --coverage
```

Coverage report for WebUI shims:
```
-------------------|---------|----------|---------|---------|-------------------
File               | % Stmts | % Branch | % Funcs | % Lines | Uncovered Line #s 
-------------------|---------|----------|---------|---------|-------------------
All files          |     100 |      100 |     100 |     100 |                   
 next-intl.ts      |     100 |      100 |     100 |     100 |                   
 next-link.tsx     |     100 |      100 |     100 |     100 |                   
 ...-navigation.ts |     100 |      100 |     100 |     100 |                   
 next-script.tsx   |     100 |      100 |     100 |     100 |                   
-------------------|---------|----------|---------|---------|-------------------
```

---

## License

MIT OR Apache-2.0
