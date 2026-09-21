# Deployment and Operations Guide for Kombu Analytics

Comprehensive deployment, containerization, raw binary installation, init system configuration, and source build instructions for Kombu Analytics.

---

## Deployment Options at a Glance

* **Container Runtimes:** Docker, Docker Compose, Colima, Podman, nerdctl (containerd), Kubernetes.
* **Precompiled Standalone Binaries:** Single static binary with embedded Web UI for 31 architectures (Linux, BSD, macOS, Windows).
* **Init Systems:** systemd, OpenRC, runit, s6, SysVinit, launchd, FreeBSD rc.d, Monit, Supervisord.
* **Ingress & Networking:** Reverse proxy (Caddy, Nginx), Cloudflare Tunnel, Tailscale, Headscale.
* **Source Compilation:** Native release builds and multi-architecture cross-compilation.

---

## 1. Automated Quick Install (Linux, BSD, macOS)

Install the latest precompiled binary and base configuration with a single command:

```bash
curl -fsSL https://raw.githubusercontent.com/edideaur/kombu-analytics/main/scripts/install.sh | sudo sh
```

The script detects operating system, CPU architecture, and libc variant (musl vs glibc), downloads the matching binary from GitHub Releases, validates the SHA256 checksum against `SHA256SUMS`, installs to `/usr/local/bin/kombu`, and generates `/etc/kombu/kombu.env`.

---

## 2. Precompiled Binaries and GitHub Releases

Every commit to `main` publishes standalone binaries with the complete Vite SPA embedded directly inside. No Node.js or external static assets are required.

### Architecture Support Matrix

| Target Triple | Platform / Architecture | Description |
|---|---|---|
| `x86_64-unknown-linux-musl` | Linux x86_64 (musl) | Static binary, all modern 64-bit Linux distributions |
| `x86_64-unknown-linux-gnu` | Linux x86_64 (glibc) | Dynamic glibc binary for Debian, Ubuntu, Fedora, Arch |
| `aarch64-unknown-linux-musl` | Linux ARM64 (musl) | Static binary for 64-bit ARM (AWS Graviton, Ampere) |
| `aarch64-unknown-linux-gnu` | Linux ARM64 (glibc) | Dynamic glibc binary for ARM64 |
| `armv7-unknown-linux-musleabihf` | Linux ARMv7 (musl, hard-float) | Raspberry Pi 2/3/4 (32-bit OS), BeagleBone |
| `armv7-unknown-linux-gnueabihf` | Linux ARMv7 (glibc, hard-float) | Raspbian 32-bit, Debian ARMHF |
| `armv7-unknown-linux-musleabi` | Linux ARMv7 (musl, soft-float) | Embedded ARMv7 platforms without FPU |
| `armv7-unknown-linux-gnueabi` | Linux ARMv7 (glibc, soft-float) | Embedded ARMv7 GNU |
| `arm-unknown-linux-musleabi` | Linux ARMv6 (musl, soft-float) | Raspberry Pi Zero, Pi 1, legacy ARM |
| `arm-unknown-linux-musleabihf` | Linux ARMv6 (musl, hard-float) | ARMv6 hard-float boards |
| `arm-unknown-linux-gnueabi` | Linux ARMv6 (glibc, soft-float) | ARMv6 GNU soft-float |
| `arm-unknown-linux-gnueabihf` | Linux ARMv6 (glibc, hard-float) | Raspbian on Raspberry Pi 1/Zero |
| `armv5te-unknown-linux-musleabi` | Linux ARMv5TE (musl) | Legacy embedded ARM devices |
| `riscv64gc-unknown-linux-gnu` | Linux RISC-V 64-bit | VisionFive, StarFive, Allwinner Nezha |
| `powerpc-unknown-linux-gnu` | Linux PowerPC 32-bit | Big-endian 32-bit PowerPC |
| `powerpc64-unknown-linux-gnu` | Linux PowerPC 64-bit | Big-endian 64-bit PowerPC |
| `powerpc64le-unknown-linux-gnu` | Linux PowerPC 64-bit LE | Little-endian POWER8, POWER9, POWER10 |
| `s390x-unknown-linux-gnu` | Linux s390x | IBM System z mainframe architectures |
| `i686-unknown-linux-musl` | Linux x86 32-bit (musl) | Static binary for 32-bit Pentium Pro and above |
| `i686-unknown-linux-gnu` | Linux x86 32-bit (glibc) | 32-bit x86 glibc distributions |
| `x86_64-unknown-freebsd` | FreeBSD x86_64 | FreeBSD 13 and 14 on 64-bit Intel and AMD |
| `i686-unknown-freebsd` | FreeBSD i686 | FreeBSD 32-bit x86 |
| `x86_64-unknown-illumos` | illumos / Solaris | OmniOS, SmartOS, OpenIndiana |
| `universal-apple-darwin` | macOS Universal Binary | Combined Intel and Apple Silicon native binary |
| `x86_64-apple-darwin` | macOS Intel | Intel Macs (x86_64) |
| `aarch64-apple-darwin` | macOS Apple Silicon | Apple Silicon M1, M2, M3, M4 Macs |
| `x86_64-pc-windows-msvc.exe` | Windows x64 (MSVC) | Native 64-bit Windows binary |
| `x86_64-pc-windows-gnu.exe` | Windows x64 (MinGW) | MinGW GCC 64-bit Windows binary |
| `i686-pc-windows-msvc.exe` | Windows 32-bit (MSVC) | 32-bit x86 Windows binary |
| `i686-pc-windows-gnu.exe` | Windows 32-bit (MinGW) | 32-bit MinGW Windows binary |
| `aarch64-pc-windows-msvc.exe` | Windows ARM64 (MSVC) | Qualcomm Snapdragon, Surface Pro ARM |

### Manual Binary Download and Verification

1. Download the target binary and the checksum manifest:

```bash
VERSION="latest"
TARGET="x86_64-unknown-linux-musl"
curl -fsSL "https://github.com/edideaur/kombu-analytics/releases/${VERSION}/download/kombu-${TARGET}" -o kombu
curl -fsSL "https://github.com/edideaur/kombu-analytics/releases/${VERSION}/download/SHA256SUMS" -o SHA256SUMS
```

2. Verify integrity:

```bash
grep "kombu-${TARGET}$" SHA256SUMS | sha256sum -c --ignore-missing
```

3. Install system-wide:

```bash
sudo install -m 755 kombu /usr/local/bin/kombu
```

4. Create dedicated service user and group:

```bash
sudo useradd --system --no-create-home --shell /bin/false kombu
```

5. Create production environment file:

```bash
sudo mkdir -p /etc/kombu
sudo tee /etc/kombu/kombu.env << 'EOF'
DATABASE_URL=postgres://kombu:kombu@localhost:5432/kombu
APP_SECRET=replace_with_32_characters_random_secret_string
LISTEN=0.0.0.0:3000
DATABASE_MAX_CONNECTIONS=80
COLLECT_RATE_LIMIT=3000
STORAGE_ENGINE=postgres
EOF
sudo chmod 600 /etc/kombu/kombu.env
sudo chown -R kombu:kombu /etc/kombu
```

---

## 3. Init System Service Configurations

### systemd (Debian, Ubuntu, Fedora, Arch, RHEL, openSUSE)

1. Copy service unit definition:

```bash
sudo cp init/systemd/kombu.service /etc/systemd/system/kombu.service
```

2. Run database migrations:

```bash
sudo -u kombu /usr/local/bin/kombu migrate --database-url "$DATABASE_URL"
```

3. Reload systemd daemon and activate service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now kombu.service
```

4. Inspect logs and health:

```bash
sudo systemctl status kombu.service
journalctl -u kombu.service -f
curl -f http://localhost:3000/api/health
```

### OpenRC (Alpine Linux, Gentoo, postmarketOS)

1. Install initd service and configuration:

```bash
sudo cp init/openrc/kombu.initd /etc/init.d/kombu
sudo cp init/openrc/kombu.confd /etc/conf.d/kombu
sudo chmod +x /etc/init.d/kombu
```

2. Enable service at default runlevel and start:

```bash
sudo rc-update add kombu default
sudo rc-service kombu start
```

3. Query service status:

```bash
sudo rc-service kombu status
```

### runit (Void Linux, Artix, runit containers)

1. Create service folder:

```bash
sudo mkdir -p /etc/sv/kombu
sudo cp init/runit/run /etc/sv/kombu/run
sudo cp init/runit/finish /etc/sv/kombu/finish
sudo chmod +x /etc/sv/kombu/run /etc/sv/kombu/finish
```

2. Enable and supervise:

```bash
sudo ln -s /etc/sv/kombu /var/service/
sudo sv status kombu
```

### s6 / s6-overlay (Alpine s6, container appliances)

1. Place service definitions:

```bash
sudo mkdir -p /etc/services.d/kombu
sudo cp init/s6/run /etc/services.d/kombu/run
sudo cp init/s6/finish /etc/services.d/kombu/finish
sudo cp init/s6/type /etc/services.d/kombu/type
sudo chmod +x /etc/services.d/kombu/run /etc/services.d/kombu/finish
```

2. Scan and supervise:

```bash
s6-rc-update /etc/s6-rc/compiled
```

### SysVinit (Debian legacy, Devuan)

1. Install init script:

```bash
sudo cp init/sysvinit/kombu /etc/init.d/kombu
sudo chmod +x /etc/init.d/kombu
sudo update-rc.d kombu defaults
```

2. Control service:

```bash
sudo /etc/init.d/kombu start
sudo /etc/init.d/kombu status
sudo /etc/init.d/kombu health
```

### macOS Native (launchd)

1. Install daemon property list:

```bash
sudo cp init/launchd/com.kombu.analytics.plist /Library/LaunchDaemons/
sudo chown root:wheel /Library/LaunchDaemons/com.kombu.analytics.plist
sudo chmod 644 /Library/LaunchDaemons/com.kombu.analytics.plist
```

2. Load and launch daemon:

```bash
sudo launchctl load -w /Library/LaunchDaemons/com.kombu.analytics.plist
```

3. Verify execution:

```bash
sudo launchctl list | grep kombu
curl -f http://localhost:3000/api/health
```

### FreeBSD rc.d

1. Install rc script:

```bash
sudo cp init/freebsd/kombu /usr/local/etc/rc.d/kombu
sudo chmod +x /usr/local/etc/rc.d/kombu
```

2. Configure in `/etc/rc.conf`:

```bash
sudo sysrc kombu_enable="YES"
sudo service kombu start
sudo service kombu status
```

### Process Watchdogs (Monit and Supervisord)

* **Monit:** `init/monit/kombu.monitrc` monitors process PID and performs HTTP queries against `http://127.0.0.1:3000/api/health`.
* **Supervisord:** `init/supervisord/kombu.conf` provides restart backoff, log rotation, and group management.

---

## 4. Container Deployments and Storage Engine Stacks

Ready-to-run Docker Compose stacks are provided under `docker/` for every supported storage architecture.

### Storage Backend Compose Stacks

Pull the official multi-architecture image (`linux/amd64` and `linux/arm64`):

```bash
docker pull ghcr.io/edideaur/kombu-analytics:latest
```

#### 1. PostgreSQL (Default Engine)

Ideal for up to 50 million events per year. Uses `docker/compose.postgres.yaml`:

```bash
docker compose -f docker/compose.postgres.yaml up -d
```

#### 2. PostgreSQL Partitioned with Hourly Rollups

Declarative monthly range partitioning on `created_at` with pre-computed `website_event_stats_hourly` rollups. Suitable for 50 million to 500 million events per year:

```bash
docker compose -f docker/compose.partitioned.yaml up -d
```

#### 3. TimescaleDB Engine

High-performance time-series hypertables with native compression policies:

```bash
docker compose -f docker/compose.timescale.yaml up -d
```

#### 4. ClickHouse Analytical Columnar Engine

Extreme throughput analytical engine storing events in ClickHouse while keeping users, websites, and teams in PostgreSQL:

```bash
docker compose -f docker/compose.clickhouse.yaml up -d
```

### Option B: Colima (macOS and Linux Virtual Machines)

Colima provides container runtimes with minimal overhead:

1. Install Colima and dependencies:

```bash
brew install colima docker docker-compose
```

2. Start the optimized virtual machine:

```bash
colima start kombu --config colima/colima.yaml
```

3. Run the automated deployment script:

```bash
./colima/start-colima.sh
```

4. Verify health status:

```bash
curl -f http://localhost:3000/api/health
```

### Option C: Rootless Podman and Podman Compose

1. Prepare environment variables:

```bash
cp podman/env.sample podman/.env
```

2. Start the rootless pod:

```bash
podman-compose -f podman/podman-compose.yml up -d
```

3. Enable user-level systemd supervision:

```bash
./podman/install-systemd-user-service
systemctl --user daemon-reload
systemctl --user enable --now kombu.service
```

### Option D: containerd and nerdctl

Run directly via containerd:

```bash
nerdctl compose -f docker/compose.yaml up -d
```

---

## 5. Building from Source

### Build Prerequisites

* **Rust Toolchain:** 1.85 or later (`rustup default stable`)
* **Node.js:** 20 or later
* **Package Manager:** pnpm 9 or npm
* **C Compiler:** gcc or clang and make (for jemalloc allocator)
* **PostgreSQL:** 14 or later

### Step-by-Step Compilation

1. Clone repository:

```bash
git clone https://github.com/edideaur/kombu-analytics.git
cd kombu-analytics
```

2. Build Web UI assets:

```bash
cd webui
pnpm install --legacy-peer-deps
pnpm build
cd ..
```

3. Compile standalone release binary:

```bash
cargo build --release --bin kombu
```

The output binary is placed at `target/release/kombu`. It contains the complete compiled frontend embedded into the executable.

4. Apply database migrations:

```bash
DATABASE_URL="postgres://kombu:kombu@localhost:5432/kombu" ./target/release/kombu migrate
```

5. Launch HTTP server:

```bash
APP_SECRET="secure_random_production_secret_32_characters_minimum" \
DATABASE_URL="postgres://kombu:kombu@localhost:5432/kombu" \
./target/release/kombu serve --listen 0.0.0.0:3000
```

### Cross-Compilation with cross

Compile for any supported foreign target from an x86_64 host:

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-musl --bin kombu
cross build --release --target riscv64gc-unknown-linux-gnu --bin kombu
```

---

## 6. Reverse Proxy Configuration

### Caddy (Recommended)

Automatic TLS certificate issuance and WebSocket forwarding:

```caddyfile
analytics.example.com {
    encode zstd gzip

    reverse_proxy 127.0.0.1:3000 {
        header_up Host {host}
        header_up X-Real-IP {remote_host}
        header_up X-Forwarded-For {remote_host}
        header_up X-Forwarded-Proto {scheme}
    }
}
```

### Nginx

```nginx
server {
    listen 80;
    server_name analytics.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name analytics.example.com;

    ssl_certificate /etc/letsencrypt/live/analytics.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/analytics.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;

        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        proxy_buffering off;
        proxy_read_timeout 60s;
    }
}
```

---

## 7. Environment Variables Reference

| Variable | Default | Required | Description |
|---|---|---|---|
| `DATABASE_URL` | None | Yes | PostgreSQL connection string (`postgres://user:pass@host:5432/db`) |
| `APP_SECRET` | None | Yes | 32-character random string used for cryptographic tokens and salts |
| `LISTEN` | `0.0.0.0:3000` | No | Interface and port to bind the HTTP server |
| `PORT` | `3000` | No | Port number (overridden if `LISTEN` is set) |
| `STORAGE_ENGINE` | `postgres` | No | Analytics storage backend (`postgres`, `partitioned`, `timescale`, `clickhouse`) |
| `CLICKHOUSE_URL` | None | No | ClickHouse HTTP endpoint (required if `STORAGE_ENGINE=clickhouse`) |
| `DATABASE_MAX_CONNECTIONS` | `80` | No | Maximum connection pool size for PostgreSQL |
| `DATABASE_MIN_CONNECTIONS` | `5` | No | Minimum idle connection pool size |
| `COLLECT_RATE_LIMIT` | `3000` | No | Maximum allowed event ingestion requests per minute per IP |
| `CLIENT_IP_HEADER` | Auto | No | Custom proxy header for client IP extraction (e.g. `CF-Connecting-IP`) |
| `DISABLE_BOT_DETECTION` | `0` | No | Set to `1` to disable automated crawler and bot traffic filtering |
| `TELEMETRY_DISABLED` | `1` | No | Disables telemetry reporting |
| `SENTRY_DSN` | None | No | Sentry endpoint for backend crash reporting |
| `CORS_ORIGIN` | `*` | No | Allowed CORS origin headers |

---

## 8. Database Maintenance and Backups

### PostgreSQL Backup

Create consistent logical backup:

```bash
pg_dump -U kombu -h localhost -F c -b -v -f "kombu-backup-$(date +%Y%m%d%H%M%S).dump" kombu
```

### PostgreSQL Restore

Restore into target database:

```bash
pg_restore -U kombu -h localhost -d kombu -v "kombu-backup.dump"
```

---

## 9. Health & Monitoring Endpoints

* `GET /api/health` returns HTTP 200 with database connection status.
* `GET /api/heartbeat` returns HTTP 200 with process timestamp.
* `GET /api/config` returns public server configuration metadata.
