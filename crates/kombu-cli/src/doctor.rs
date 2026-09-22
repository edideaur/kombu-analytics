#![forbid(unsafe_code)]
use anyhow::Context;
use sqlx::PgPool;

#[derive(Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub db_connected: bool,
    pub db_latency_ms: u128,
    pub pg_version: String,
    pub migrations_count: i64,
    pub storage_engine: String,
    pub timescale_installed: bool,
    pub clickhouse_status: Option<String>,
    pub geoip_status: String,
    pub website_count: i64,
    pub user_count: i64,
    pub team_count: i64,
    pub session_count: i64,
    pub event_count: i64,
}

impl DoctorReport {
    pub fn print_summary(&self) {
        println!("============================================================");
        println!("                 KOMBU SYSTEM HEALTH REPORT                 ");
        println!("============================================================");
        println!(
            " Database Status:        {} (Latency: {} ms)",
            if self.db_connected { "[OK] CONNECTED" } else { "[FAIL] DISCONNECTED" },
            self.db_latency_ms
        );
        println!(" PostgreSQL Version:     {}", self.pg_version.trim());
        println!(" Applied Migrations:     {} migrations", self.migrations_count);
        println!(" Active Storage Engine:  {}", self.storage_engine);
        println!(
            " TimescaleDB Extension:  {}",
            if self.timescale_installed { "[OK] Installed" } else { "[INFO] Not installed" }
        );
        if let Some(ref ch) = self.clickhouse_status {
            println!(" ClickHouse Backend:     {ch}");
        }
        println!(" GeoIP MaxMind Status:   {}", self.geoip_status);
        println!("------------------------------------------------------------");
        println!(" System Entity Counts:");
        println!("   - Websites:           {}", self.website_count);
        println!("   - Users:              {}", self.user_count);
        println!("   - Teams:              {}", self.team_count);
        println!("   - Sessions:           {}", self.session_count);
        println!("   - Events:             {}", self.event_count);
        println!("============================================================");
    }
}

pub async fn run_doctor(
    pool: &PgPool,
    clickhouse_url: Option<&str>,
) -> anyhow::Result<DoctorReport> {
    let start = std::time::Instant::now();
    let version_row: (String,) = sqlx::query_as("SELECT version()")
        .fetch_one(pool)
        .await
        .context("Failed to query PostgreSQL version")?;
    let db_latency_ms = start.elapsed().as_millis();

    let migrations_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let website_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM \"website\" WHERE deleted_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    let user_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM \"user\" WHERE deleted_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    let team_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"team\"")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"session\"")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"website_event\"")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let timescale_installed = kombu_db::timescale::is_timescale_installed(pool)
        .await
        .unwrap_or(false);

    let storage_engine = std::env::var("STORAGE_ENGINE")
        .or_else(|_| std::env::var("ANALYTICS_STORAGE_ENGINE"))
        .unwrap_or_else(|_| {
            if clickhouse_url.is_some() || std::env::var("CLICKHOUSE_URL").is_ok() {
                "clickhouse".to_string()
            } else if timescale_installed {
                "timescale".to_string()
            } else {
                "postgres".to_string()
            }
        });

    let env_ch = std::env::var("CLICKHOUSE_URL").ok();
    let effective_ch = clickhouse_url.or(env_ch.as_deref());

    let clickhouse_status = if let Some(ch_url) = effective_ch {
        match kombu_db::ClickHouseClient::from_url(ch_url) {
            Ok(client) => match client.ping().await {
                Ok(true) => Some(format!("[OK] Connected ({})", client.config().endpoint)),
                Ok(false) => {
                    Some(format!("[WARN] Ping failed ({})", client.config().endpoint))
                }
                Err(e) => Some(format!("[WARN] Failed to connect: {e}")),
            },
            Err(e) => Some(format!("[WARN] Invalid ClickHouse URL: {e}")),
        }
    } else {
        None
    };

    let geoip_status = if std::path::Path::new("maxmind/extracted/GeoLite2-City.mmdb").exists() {
        "[OK] Local GeoLite2-City.mmdb database active".to_string()
    } else {
        "[INFO] Fallback / embedded lookup active".to_string()
    };

    Ok(DoctorReport {
        db_connected: true,
        db_latency_ms,
        pg_version: version_row.0,
        migrations_count,
        storage_engine,
        timescale_installed,
        clickhouse_status,
        geoip_status,
        website_count,
        user_count,
        team_count,
        session_count,
        event_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_doctor_execution() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        if let Ok(pool) = PgPool::connect(&db_url).await {
            let report = run_doctor(&pool, None).await.expect("doctor run ok");
            assert!(report.db_connected);
            assert!(!report.pg_version.is_empty());
            report.print_summary();
        }
    }
}
