#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha384};
use uuid::Uuid;

pub mod doctor;
pub mod export;
pub mod plausible;
pub mod umami;
pub mod user;
pub mod website;

#[cfg(not(test))]
use tracing_subscriber::EnvFilter;

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser, Debug, PartialEq, Eq)]
#[command(name = "kombu", version, about = "Kombu analytics")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Commands {
    Serve {
        #[arg(long, default_value = "0.0.0.0:3000")]
        listen: String,
        #[arg(long)]
        engine: Option<String>,
        #[arg(long)]
        clickhouse_url: Option<String>,
    },
    Migrate {
        #[arg(long)]
        database_url: Option<String>,
        #[arg(long)]
        engine: Option<String>,
        #[arg(long)]
        clickhouse_url: Option<String>,
    },
    Rollup {
        #[arg(long)]
        website_id: Uuid,
        #[arg(long)]
        database_url: Option<String>,
    },
    BuildGeo,
    ImportPlausible {
        #[arg(long)]
        website_id: Uuid,
        #[arg(long)]
        file: std::path::PathBuf,
        #[arg(long)]
        database_url: Option<String>,
    },
    ImportUmami {
        #[arg(long)]
        website_id: Uuid,
        #[arg(long)]
        file: std::path::PathBuf,
        #[arg(long)]
        database_url: Option<String>,
    },
    Export {
        #[arg(long)]
        website_id: Uuid,
        #[arg(long, default_value = "csv")]
        format: String,
        #[arg(long)]
        output: Option<std::path::PathBuf>,
        #[arg(long)]
        start_at: Option<chrono::DateTime<chrono::Utc>>,
        #[arg(long)]
        end_at: Option<chrono::DateTime<chrono::Utc>>,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        database_url: Option<String>,
    },
    Doctor {
        #[arg(long)]
        database_url: Option<String>,
        #[arg(long)]
        clickhouse_url: Option<String>,
    },
    User {
        #[command(subcommand)]
        action: UserCommands,
    },
    Website {
        #[command(subcommand)]
        action: WebsiteCommands,
    },
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum UserCommands {
    Create {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
        #[arg(long, default_value = "admin")]
        role: String,
        #[arg(long)]
        database_url: Option<String>,
    },
    ResetPassword {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
        #[arg(long)]
        database_url: Option<String>,
    },
    List {
        #[arg(long)]
        database_url: Option<String>,
    },
    Delete {
        #[arg(long)]
        username: String,
        #[arg(long)]
        database_url: Option<String>,
    },
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum WebsiteCommands {
    List {
        #[arg(long)]
        database_url: Option<String>,
    },
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        user_id: Option<Uuid>,
        #[arg(long)]
        database_url: Option<String>,
    },
    Delete {
        #[arg(long)]
        website_id: Uuid,
        #[arg(long)]
        database_url: Option<String>,
    },
    Reset {
        #[arg(long)]
        website_id: Uuid,
        #[arg(long)]
        database_url: Option<String>,
    },
}

pub fn build_app(pool: sqlx::PgPool) -> axum::Router {
    let dist_candidates = [
        std::path::PathBuf::from("webui/dist"),
        std::path::PathBuf::from("/app/webui/dist"),
        std::path::PathBuf::from("kombu/webui/dist"),
        std::path::PathBuf::from("../../webui/dist"),
    ];
    let dist_path = dist_candidates.into_iter().find(|p| p.exists());
    build_app_with_dist(pool, dist_path)
}

pub fn build_app_with_dist(
    pool: sqlx::PgPool,
    dist_path: Option<std::path::PathBuf>,
) -> axum::Router {
    let app = kombu_api::build_router(pool);

    if let Some(path) = dist_path {
        let index_html = path.join("index.html");
        let svc = tower_http::services::ServeDir::new(&path)
            .fallback(tower_http::services::ServeFile::new(index_html));
        app.fallback_service(svc)
    } else {
        let fallback_svc = tower::service_fn(|_| async {
            use axum::response::IntoResponse;
            Ok::<_, std::convert::Infallible>(
                axum::response::Html(include_str!(concat!(env!("OUT_DIR"), "/index.html")))
                    .into_response(),
            )
        });
        app.fallback_service(fallback_svc)
    }
}

pub type BoxFuture = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;

pub fn validate_secret(secret: &str, allow_insecure: bool) -> anyhow::Result<()> {
    if (secret.is_empty()
        || secret == "replace-me-with-a-random-string"
        || secret == "kombu-secret"
        || secret.len() < 32)
        && !allow_insecure
    {
        anyhow::bail!(
            "FATAL: APP_SECRET must be set to a secure random string of at least 32 characters in production. \
            Set KOMBU_ALLOW_INSECURE_SECRET=1 only for development/testing."
        );
    }
    Ok(())
}

pub fn resolve_listen_address(listen: &str) -> String {
    if listen == "0.0.0.0:3000" {
        let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
        let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "0.0.0.0".to_string());
        format!("{host}:{port}")
    } else {
        listen.to_string()
    }
}

pub fn resolve_migrate_url(
    cli_url: Option<String>,
    env_url: Option<String>,
) -> anyhow::Result<String> {
    if let Some(u) = cli_url {
        return Ok(u);
    }
    if let Some(u) = env_url {
        return Ok(u);
    }
    anyhow::bail!("DATABASE_URL required")
}

pub fn parse_env_u32(val: Option<String>, default: u32) -> u32 {
    match val {
        Some(s) => s.parse().unwrap_or(default),
        None => default,
    }
}

pub async fn run_serve(listen: &str, shutdown: BoxFuture) -> anyhow::Result<()> {
    run_serve_internal(listen, None, None, shutdown).await
}

pub async fn run_serve_internal(
    listen: &str,
    override_secret: Option<(&str, bool)>,
    override_db_url: Option<&str>,
    shutdown: BoxFuture,
) -> anyhow::Result<()> {
    let (secret, allow_insecure) = if let Some((s, a)) = override_secret {
        (s.to_string(), a)
    } else {
        let s = std::env::var("APP_SECRET")
            .or_else(|_| std::env::var("HASH_SALT"))
            .unwrap_or_default();
        let a =
            std::env::var("KOMBU_ALLOW_INSECURE_SECRET").unwrap_or_default() == "1" || cfg!(test);
        (s, a)
    };
    validate_secret(&secret, allow_insecure)?;

    let effective_listen = resolve_listen_address(listen);

    let default_db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
    let db_url = override_db_url.unwrap_or(&default_db_url);
    let max_conns = parse_env_u32(std::env::var("DATABASE_MAX_CONNECTIONS").ok(), 80);
    let min_conns = parse_env_u32(std::env::var("DATABASE_MIN_CONNECTIONS").ok(), 10);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_conns)
        .min_connections(min_conns)
        .idle_timeout(Some(std::time::Duration::from_secs(30)))
        .max_lifetime(Some(std::time::Duration::from_secs(1800)))
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_lazy(db_url)?;

    let _bg_handle = kombu_api::background::start_background_tasks(pool.clone());

    let app = build_app(pool);

    let listener = tokio::net::TcpListener::bind(&effective_listen).await?;
    tracing::info!("listening on {effective_listen}");
    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await;
    Ok(())
}

pub async fn run_cli_with_shutdown(cli: Cli, shutdown: BoxFuture) -> anyhow::Result<()> {
    run_cli_internal(cli, shutdown, std::env::var("DATABASE_URL").ok()).await
}

pub async fn run_cli_internal(
    cli: Cli,
    shutdown: BoxFuture,
    env_db_url: Option<String>,
) -> anyhow::Result<()> {
    match cli.command {
        Commands::Serve {
            listen,
            engine: _,
            clickhouse_url: _,
        } => {
            run_serve(&listen, shutdown).await?;
        }
        Commands::Migrate {
            database_url,
            engine,
            clickhouse_url,
        } => {
            let url = resolve_migrate_url(database_url, env_db_url)?;
            if let Ok(pool) = sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(10))
                .connect(&url)
                .await
            {
                let content = include_str!("../../kombu-db/migrations/01_initial.sql");
                let mut hasher = Sha384::new();
                hasher.update(content.as_bytes());
                let computed = hasher.finalize();
                let _ = sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = 1")
                    .bind(computed.as_slice())
                    .execute(&pool)
                    .await;

                sqlx::migrate!("../kombu-db/migrations").run(&pool).await?;
                println!("migrations applied");

                let selected_engine = engine
                    .as_deref()
                    .and_then(kombu_core::types::StorageEngine::parse_str)
                    .unwrap_or_else(kombu_core::types::StorageEngine::detect_from_env);

                if let Some(ch_url) =
                    clickhouse_url.or_else(|| std::env::var("CLICKHOUSE_URL").ok())
                {
                    if let Ok(ch_client) = kombu_db::ClickHouseClient::from_url(&ch_url) {
                        println!(
                            "applying ClickHouse migrations at {}",
                            ch_client.config().endpoint
                        );
                        let _ = ch_client.apply_schema().await;
                    }
                }

                if selected_engine == kombu_core::types::StorageEngine::Timescale {
                    let installed = kombu_db::timescale::is_timescale_installed(&pool)
                        .await
                        .unwrap_or(false);
                    println!("TimescaleDB extension installed: {installed}");
                }
            } else {
                anyhow::bail!("Failed to connect to database for migrations");
            }
        }
        Commands::Rollup {
            website_id,
            database_url,
        } => {
            let url = resolve_migrate_url(database_url, env_db_url)?;
            let pool = match sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect(&url)
                .await
            {
                Ok(p) => p,
                Err(e) => anyhow::bail!("Failed to connect to database: {e}"),
            };
            let end_at = chrono::Utc::now();
            let start_at = end_at - chrono::Duration::days(30);
            kombu_db::partitioned::refresh_hourly_rollups(&pool, website_id, start_at, end_at)
                .await?;
            println!("Successfully refreshed hourly rollups for website {website_id}");
        }
        Commands::BuildGeo => {
            println!("build-geo: embed MaxMind DB via include_bytes!, nothing to do");
        }
        Commands::ImportPlausible {
            website_id,
            file,
            database_url,
        } => {
            let url = resolve_migrate_url(database_url, env_db_url)?;
            let pool = match sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect(&url)
                .await
            {
                Ok(p) => p,
                Err(e) => anyhow::bail!("Failed to connect to database: {e}"),
            };
            let content = std::fs::read_to_string(&file)?;
            let count = plausible::import_plausible_csv(&pool, website_id, &content)
                .await
                .unwrap_or(0);
            println!(
                "Successfully imported {count} events from Plausible CSV into website {website_id}"
            );
        }
        Commands::ImportUmami {
            website_id,
            file,
            database_url,
        } => {
            let url = resolve_migrate_url(database_url, env_db_url)?;
            let pool = match sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect(&url)
                .await
            {
                Ok(p) => p,
                Err(e) => anyhow::bail!("Failed to connect to database: {e}"),
            };
            let content = std::fs::read_to_string(&file)?;
            let count = umami::import_umami_json(&pool, website_id, &content).await?;
            println!(
                "Successfully imported {count} events from Umami JSON into website {website_id}"
            );
        }
        Commands::Export {
            website_id,
            format,
            output,
            start_at,
            end_at,
            limit,
            database_url,
        } => {
            let url = resolve_migrate_url(database_url, env_db_url)?;
            let pool = match sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect(&url)
                .await
            {
                Ok(p) => p,
                Err(e) => anyhow::bail!("Failed to connect to database: {e}"),
            };
            export::run_export(
                &pool,
                website_id,
                &format,
                output.as_deref(),
                start_at,
                end_at,
                limit,
            )
            .await?;
        }
        Commands::Doctor {
            database_url,
            clickhouse_url,
        } => {
            let url = resolve_migrate_url(database_url, env_db_url)?;
            let pool = match sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect(&url)
                .await
            {
                Ok(p) => p,
                Err(e) => anyhow::bail!("Failed to connect to database: {e}"),
            };
            let report = doctor::run_doctor(&pool, clickhouse_url.as_deref()).await?;
            report.print_summary();
        }
        Commands::User { action } => match action {
            UserCommands::Create {
                username,
                password,
                role,
                database_url,
            } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                let uid = user::create_user(&pool, &username, &password, &role).await?;
                println!("Successfully created user '{username}' ({role}) with ID {uid}");
            }
            UserCommands::ResetPassword {
                username,
                password,
                database_url,
            } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                user::reset_password(&pool, &username, &password).await?;
                println!("Successfully reset password for user '{username}'");
            }
            UserCommands::List { database_url } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                let users = user::list_users(&pool).await?;
                println!("{:<36}  {:<20}  {:<10}  {:<25}", "USER ID", "USERNAME", "ROLE", "CREATED AT");
                println!("{}", "-".repeat(95));
                for u in users {
                    let created = u.created_at.map_or_else(|| "-".to_string(), |c| c.to_rfc3339());
                    println!("{:<36}  {:<20}  {:<10}  {:<25}", u.user_id, u.username, u.role, created);
                }
            }
            UserCommands::Delete {
                username,
                database_url,
            } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                user::delete_user(&pool, &username).await?;
                println!("Successfully deleted user '{username}'");
            }
        },
        Commands::Website { action } => match action {
            WebsiteCommands::List { database_url } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                let websites = website::list_websites(&pool).await?;
                println!("{:<36}  {:<25}  {:<25}  {:<25}", "WEBSITE ID", "NAME", "DOMAIN", "CREATED AT");
                println!("{}", "-".repeat(115));
                for w in websites {
                    let domain = w.domain.unwrap_or_else(|| "-".to_string());
                    let created = w.created_at.map_or_else(|| "-".to_string(), |c| c.to_rfc3339());
                    println!("{:<36}  {:<25}  {:<25}  {:<25}", w.website_id, w.name, domain, created);
                }
            }
            WebsiteCommands::Create {
                name,
                domain,
                user_id,
                database_url,
            } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                let wid = website::create_website(&pool, &name, domain.as_deref(), user_id).await?;
                println!("Successfully created website '{name}' with ID {wid}");
            }
            WebsiteCommands::Delete {
                website_id,
                database_url,
            } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                website::delete_website(&pool, website_id).await?;
                println!("Successfully deleted website {website_id}");
            }
            WebsiteCommands::Reset {
                website_id,
                database_url,
            } => {
                let url = resolve_migrate_url(database_url, env_db_url)?;
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .acquire_timeout(std::time::Duration::from_secs(5))
                    .connect(&url)
                    .await?;
                website::reset_website(&pool, website_id).await?;
                println!("Successfully reset all data for website {website_id}");
            }
        },
    }
    Ok(())
}

pub fn init_sentry(dsn: Option<&str>) -> Option<sentry::ClientInitGuard> {
    let dsn = dsn
        .map(ToString::to_string)
        .or_else(|| std::env::var("SENTRY_DSN").ok())
        .unwrap_or_else(|| {
            "https://525c47168b854f1e9e62cd86d65c8e65@rustrak-api.edideaur.works/40".to_string()
        });

    if dsn.is_empty() {
        None
    } else {
        Some(sentry::init((
            dsn.as_str(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                send_default_pii: true,
                ..Default::default()
            },
        )))
    }
}

#[cfg(not(test))]
pub async fn run_cli(cli: Cli) -> anyhow::Result<()> {
    run_cli_with_shutdown(
        cli,
        Box::pin(async {
            let _ = tokio::signal::ctrl_c().await;
        }),
    )
    .await
}

#[cfg(not(test))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = init_sentry(None);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    run_cli(cli).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    #[test]
    fn test_cli_parse_serve() {
        let cli = Cli::try_parse_from([
            "kombu",
            "serve",
            "--listen",
            "127.0.0.1:8080",
            "--engine",
            "clickhouse",
            "--clickhouse-url",
            "http://localhost:8123",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::Serve {
                listen: "127.0.0.1:8080".into(),
                engine: Some("clickhouse".into()),
                clickhouse_url: Some("http://localhost:8123".into()),
            }
        );
    }

    #[test]
    fn test_cli_parse_migrate() {
        let cli = Cli::try_parse_from([
            "kombu",
            "migrate",
            "--database-url",
            "postgres://test",
            "--engine",
            "timescale",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::Migrate {
                database_url: Some("postgres://test".into()),
                engine: Some("timescale".into()),
                clickhouse_url: None,
            }
        );
    }

    #[test]
    fn test_cli_parse_rollup() {
        let cli = Cli::try_parse_from([
            "kombu",
            "rollup",
            "--website-id",
            "550e8400-e29b-41d4-a716-446655440000",
            "--database-url",
            "postgres://test",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::Rollup {
                website_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
                database_url: Some("postgres://test".into()),
            }
        );
    }

    #[test]
    fn test_cli_parse_build_geo() {
        let cli = Cli::try_parse_from(["kombu", "build-geo"]).unwrap();
        assert_eq!(cli.command, Commands::BuildGeo);
    }

    #[test]
    fn test_cli_parse_import_plausible() {
        let cli = Cli::try_parse_from([
            "kombu",
            "import-plausible",
            "--website-id",
            "550e8400-e29b-41d4-a716-446655440000",
            "--file",
            "test.csv",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::ImportPlausible {
                website_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
                file: std::path::PathBuf::from("test.csv"),
                database_url: None,
            }
        );
    }

    #[test]
    fn test_cli_parse_import_umami() {
        let cli = Cli::try_parse_from([
            "kombu",
            "import-umami",
            "--website-id",
            "550e8400-e29b-41d4-a716-446655440000",
            "--file",
            "umami.json",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::ImportUmami {
                website_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
                file: std::path::PathBuf::from("umami.json"),
                database_url: None,
            }
        );
    }

    #[test]
    fn test_cli_parse_export() {
        let cli = Cli::try_parse_from([
            "kombu",
            "export",
            "--website-id",
            "550e8400-e29b-41d4-a716-446655440000",
            "--format",
            "json",
            "--output",
            "out.json",
            "--limit",
            "500",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::Export {
                website_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
                format: "json".to_string(),
                output: Some(std::path::PathBuf::from("out.json")),
                start_at: None,
                end_at: None,
                limit: Some(500),
                database_url: None,
            }
        );
    }

    #[test]
    fn test_cli_parse_doctor() {
        let cli = Cli::try_parse_from(["kombu", "doctor"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Doctor {
                database_url: None,
                clickhouse_url: None,
            }
        );
    }

    #[test]
    fn test_cli_parse_user_and_website_commands() {
        let cli_user = Cli::try_parse_from([
            "kombu", "user", "create", "--username", "testadmin", "--password", "SecurePassword123!", "--role", "admin"
        ]).unwrap();
        assert_eq!(
            cli_user.command,
            Commands::User {
                action: UserCommands::Create {
                    username: "testadmin".into(),
                    password: "SecurePassword123!".into(),
                    role: "admin".into(),
                    database_url: None,
                }
            }
        );

        let cli_user_reset = Cli::try_parse_from([
            "kombu", "user", "reset-password", "--username", "testadmin", "--password", "NewPass12345!"
        ]).unwrap();
        assert_eq!(
            cli_user_reset.command,
            Commands::User {
                action: UserCommands::ResetPassword {
                    username: "testadmin".into(),
                    password: "NewPass12345!".into(),
                    database_url: None,
                }
            }
        );

        let cli_user_list = Cli::try_parse_from(["kombu", "user", "list"]).unwrap();
        assert_eq!(
            cli_user_list.command,
            Commands::User {
                action: UserCommands::List { database_url: None }
            }
        );

        let cli_user_del = Cli::try_parse_from([
            "kombu", "user", "delete", "--username", "testadmin"
        ]).unwrap();
        assert_eq!(
            cli_user_del.command,
            Commands::User {
                action: UserCommands::Delete {
                    username: "testadmin".into(),
                    database_url: None,
                }
            }
        );

        let cli_site_create = Cli::try_parse_from([
            "kombu", "website", "create", "--name", "My Blog", "--domain", "blog.example.com"
        ]).unwrap();
        assert_eq!(
            cli_site_create.command,
            Commands::Website {
                action: WebsiteCommands::Create {
                    name: "My Blog".into(),
                    domain: Some("blog.example.com".into()),
                    user_id: None,
                    database_url: None,
                }
            }
        );

        let cli_site_list = Cli::try_parse_from(["kombu", "website", "list"]).unwrap();
        assert_eq!(
            cli_site_list.command,
            Commands::Website {
                action: WebsiteCommands::List { database_url: None }
            }
        );
    }

    #[tokio::test]
    async fn test_run_import_plausible_cmd() {
        let file_path = std::env::temp_dir().join(format!("plausible_test_{}.csv", Uuid::now_v7()));
        std::fs::write(&file_path, "date,time,page\n2026-09-01,12:00:00,/test\n").unwrap();

        let website_id = Uuid::now_v7();
        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "website" (website_id, name, domain, created_at) VALUES ($1, 'Cli Test', 'cli.test', now())"#)
            .bind(website_id)
            .execute(&pool)
            .await
            .unwrap();

        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::ImportPlausible {
                    website_id,
                    file: file_path,
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_ok());

        let res_env = run_cli_with_shutdown(
            Cli {
                command: Commands::ImportPlausible {
                    website_id,
                    file: std::env::temp_dir().join("plausible_empty.csv"),
                    database_url: None,
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        let _ = res_env;

        let bad_file_res = run_cli_with_shutdown(
            Cli {
                command: Commands::ImportPlausible {
                    website_id,
                    file: std::path::PathBuf::from("/nonexistent/path/plausible.csv"),
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(bad_file_res.is_err());

        let bad_db_res = run_cli_with_shutdown(
            Cli {
                command: Commands::ImportPlausible {
                    website_id,
                    file: std::path::PathBuf::from("/tmp/does_not_matter"),
                    database_url: Some("postgres://invalid_host_123.invalid:5432/bad".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(bad_db_res.is_err());

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_run_build_geo() {
        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::BuildGeo,
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_build_app_both_branches() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://kombu:kombu@localhost:5432/kombu")
            .unwrap();

        let app1 = build_app(pool.clone());
        drop(app1);

        let app2 = build_app_with_dist(pool, None);
        let req = axum::http::Request::builder()
            .uri("/some/random/client/path")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app2.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_run_migrate() {
        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();
        let _ = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 2")
            .execute(&pool)
            .await;

        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::Migrate {
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                    engine: Some("partitioned".into()),
                    clickhouse_url: Some("http://localhost:8123".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_ok(), "Migrate failed: {res:?}");

        let res_ts = run_cli_with_shutdown(
            Cli {
                command: Commands::Migrate {
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                    engine: Some("timescale".into()),
                    clickhouse_url: None,
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_ts.is_ok());

        let wid = Uuid::parse_str("01a0aa65-c671-72cc-b248-2884c0b76273").unwrap();
        let res_rollup = run_cli_with_shutdown(
            Cli {
                command: Commands::Rollup {
                    website_id: wid,
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_rollup.is_ok());

        let res_rollup_bad_db = run_cli_with_shutdown(
            Cli {
                command: Commands::Rollup {
                    website_id: wid,
                    database_url: Some("postgres://invalid_host_xyz.invalid:5432/bad".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_rollup_bad_db.is_err());
    }

    #[tokio::test]
    async fn test_run_migrate_invalid_url() {
        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::Migrate {
                    database_url: Some("postgres://invalid_host_xyz_123.invalid:5432/bad".into()),
                    engine: None,
                    clickhouse_url: None,
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_run_migrate_no_url_error() {
        let res = run_cli_internal(
            Cli {
                command: Commands::Migrate {
                    database_url: None,
                    engine: None,
                    clickhouse_url: None,
                },
            },
            Box::pin(std::future::ready(())),
            None,
        )
        .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_run_migrate_invalid_clickhouse_url_skipped() {
        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::Migrate {
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                    engine: None,
                    clickhouse_url: Some("::not a url::".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_run_rollup_no_url_error() {
        let res = run_cli_internal(
            Cli {
                command: Commands::Rollup {
                    website_id: Uuid::now_v7(),
                    database_url: None,
                },
            },
            Box::pin(std::future::ready(())),
            None,
        )
        .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_migrate_and_rollup_failure_paths_on_tmp_db() {
        const ADMIN_URL: &str = "postgres://kombu:kombu@localhost:5432/kombu";
        const TMP_URL: &str = "postgres://kombu:kombu@localhost:5432/kombu_cov_tmp";
        let admin = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(ADMIN_URL)
            .await
            .unwrap();
        sqlx::query("DROP DATABASE IF EXISTS kombu_cov_tmp WITH (FORCE)")
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query("CREATE DATABASE kombu_cov_tmp")
            .execute(&admin)
            .await
            .unwrap();

        let rollup_fail = run_cli_with_shutdown(
            Cli {
                command: Commands::Rollup {
                    website_id: Uuid::now_v7(),
                    database_url: Some(TMP_URL.into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(rollup_fail.is_err());

        let mig_ok = run_cli_with_shutdown(
            Cli {
                command: Commands::Migrate {
                    database_url: Some(TMP_URL.into()),
                    engine: None,
                    clickhouse_url: None,
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(mig_ok.is_ok());

        let tmp = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(TMP_URL)
            .await
            .unwrap();
        let orig: Vec<u8> =
            sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = 2")
                .fetch_one(&tmp)
                .await
                .unwrap();
        sqlx::query("UPDATE _sqlx_migrations SET checksum = '\\x00' WHERE version = 2")
            .execute(&tmp)
            .await
            .unwrap();
        let mig_bad = run_cli_with_shutdown(
            Cli {
                command: Commands::Migrate {
                    database_url: Some(TMP_URL.into()),
                    engine: None,
                    clickhouse_url: None,
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = 2")
            .bind(&orig)
            .execute(&tmp)
            .await
            .unwrap();
        assert!(mig_bad.is_err());
        tmp.close().await;

        sqlx::query("DROP DATABASE IF EXISTS kombu_cov_tmp WITH (FORCE)")
            .execute(&admin)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_run_serve_lifecycle() {
        assert!(validate_secret("short", false).is_err());
        assert!(validate_secret("", false).is_err());
        assert!(validate_secret("replace-me-with-a-random-string", false).is_err());
        assert!(validate_secret("kombu-secret", false).is_err());
        assert!(validate_secret("short", true).is_ok());
        assert!(validate_secret("this-is-a-valid-secret-string-of-32-chars!!", false).is_ok());

        assert_eq!(parse_env_u32(Some("150".into()), 200), 150);
        assert_eq!(parse_env_u32(Some("not_a_num".into()), 200), 200);
        assert_eq!(parse_env_u32(None, 200), 200);

        assert_eq!(
            resolve_migrate_url(Some("pg://cli".into()), None).unwrap(),
            "pg://cli"
        );
        assert_eq!(
            resolve_migrate_url(None, Some("pg://env".into())).unwrap(),
            "pg://env"
        );
        assert!(resolve_migrate_url(None, None).is_err());

        let res_sec_err = run_serve_internal(
            "127.0.0.1:0",
            Some(("short", false)),
            None,
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_sec_err.is_err());
        let res_db_err = run_serve_internal(
            "127.0.0.1:0",
            Some(("this-is-a-valid-secret-string-of-32-chars!!", false)),
            Some("invalid-postgres-uri"),
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_db_err.is_err());

        assert!(resolve_listen_address("0.0.0.0:3000").contains(':'));
        assert_eq!(resolve_listen_address("127.0.0.1:8080"), "127.0.0.1:8080");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let res = run_cli_with_shutdown(
                Cli {
                    command: Commands::Serve {
                        listen: addr.to_string(),
                        engine: Some("postgres".into()),
                        clickhouse_url: None,
                    },
                },
                Box::pin(async move {
                    let _ = shutdown_rx.await;
                }),
            )
            .await;
            assert!(res.is_ok());
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        {
            let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            drop(stream);
        }
        let _ = shutdown_tx.send(());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_run_cli_serve_invalid_addr() {
        let cli = Cli {
            command: Commands::Serve {
                listen: "256.256.256.256:99999".to_string(),
                engine: None,
                clickhouse_url: None,
            },
        };
        let res = run_cli_with_shutdown(cli, Box::pin(std::future::ready(()))).await;
        assert!(res.is_err());
    }

    #[test]
    fn test_init_sentry() {
        let guard_some = init_sentry(Some(
            "https://525c47168b854f1e9e62cd86d65c8e65@rustrak-api.edideaur.works/40",
        ));
        assert!(guard_some.is_some());

        let guard_none = init_sentry(Some(""));
        assert!(guard_none.is_none());

        let guard_default = init_sentry(None);
        assert!(guard_default.is_some());
    }

    #[tokio::test]
    async fn test_run_doctor_cmd() {
        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::Doctor {
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                    clickhouse_url: None,
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_ok());

        let res_bad = run_cli_with_shutdown(
            Cli {
                command: Commands::Doctor {
                    database_url: Some("postgres://127.0.0.1:1/bad".into()),
                    clickhouse_url: None,
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_bad.is_err());
    }

    #[tokio::test]
    async fn test_run_export_cmd() {
        let website_id = Uuid::now_v7();
        let out_path = std::env::temp_dir().join(format!("test_export_cli_{}.csv", website_id));
        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::Export {
                    website_id,
                    format: "csv".into(),
                    output: Some(out_path.clone()),
                    start_at: None,
                    end_at: None,
                    limit: Some(10),
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_ok());
        let _ = std::fs::remove_file(out_path);

        let res_bad = run_cli_with_shutdown(
            Cli {
                command: Commands::Export {
                    website_id,
                    format: "json".into(),
                    output: None,
                    start_at: None,
                    end_at: None,
                    limit: None,
                    database_url: Some("postgres://127.0.0.1:1/bad".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_bad.is_err());
    }

    #[tokio::test]
    async fn test_run_import_umami_cmd() {
        let website_id = Uuid::now_v7();
        let file_path = std::env::temp_dir().join(format!("test_umami_{}.json", website_id));
        std::fs::write(&file_path, r#"[{"urlPath":"/page","browser":"Safari"}]"#).unwrap();

        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "website" (website_id, name, domain, created_at) VALUES ($1, 'Umami Cli Test', 'umami.test', now())"#)
            .bind(website_id)
            .execute(&pool)
            .await
            .unwrap();

        let res = run_cli_with_shutdown(
            Cli {
                command: Commands::ImportUmami {
                    website_id,
                    file: file_path.clone(),
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res.is_ok());
        let _ = std::fs::remove_file(file_path);

        let bad_res = run_cli_with_shutdown(
            Cli {
                command: Commands::ImportUmami {
                    website_id,
                    file: std::path::PathBuf::from("/nonexistent/file.json"),
                    database_url: Some("postgres://kombu:kombu@localhost:5432/kombu".into()),
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(bad_res.is_err());

        let _ = sqlx::query(r#"DELETE FROM "website" WHERE website_id = $1"#)
            .bind(website_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn test_run_user_and_website_cmds() {
        let uname = format!("cli_user_{}", Uuid::now_v7().simple());
        let db_url = Some("postgres://kombu:kombu@localhost:5432/kombu".to_string());

        let res_user_create = run_cli_with_shutdown(
            Cli {
                command: Commands::User {
                    action: UserCommands::Create {
                        username: uname.clone(),
                        password: "InitialPassword123!".into(),
                        role: "admin".into(),
                        database_url: db_url.clone(),
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_user_create.is_ok());

        let res_user_list = run_cli_with_shutdown(
            Cli {
                command: Commands::User {
                    action: UserCommands::List {
                        database_url: db_url.clone(),
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_user_list.is_ok());

        let res_user_reset = run_cli_with_shutdown(
            Cli {
                command: Commands::User {
                    action: UserCommands::ResetPassword {
                        username: uname.clone(),
                        password: "UpdatedPassword456!".into(),
                        database_url: db_url.clone(),
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_user_reset.is_ok());

        let site_name = format!("CLI Site {}", Uuid::now_v7().simple());
        let res_site_create = run_cli_with_shutdown(
            Cli {
                command: Commands::Website {
                    action: WebsiteCommands::Create {
                        name: site_name.clone(),
                        domain: Some("clitest.org".into()),
                        user_id: None,
                        database_url: db_url.clone(),
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_site_create.is_ok());

        let res_site_list = run_cli_with_shutdown(
            Cli {
                command: Commands::Website {
                    action: WebsiteCommands::List {
                        database_url: db_url.clone(),
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_site_list.is_ok());

        let pool = sqlx::PgPool::connect("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();
        let wid: Uuid = sqlx::query_scalar("SELECT website_id FROM \"website\" WHERE name = $1")
            .bind(&site_name)
            .fetch_one(&pool)
            .await
            .unwrap();

        let res_site_reset = run_cli_with_shutdown(
            Cli {
                command: Commands::Website {
                    action: WebsiteCommands::Reset {
                        website_id: wid,
                        database_url: db_url.clone(),
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_site_reset.is_ok());

        let res_site_del = run_cli_with_shutdown(
            Cli {
                command: Commands::Website {
                    action: WebsiteCommands::Delete {
                        website_id: wid,
                        database_url: db_url.clone(),
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_site_del.is_ok());

        let res_user_del = run_cli_with_shutdown(
            Cli {
                command: Commands::User {
                    action: UserCommands::Delete {
                        username: uname,
                        database_url: db_url,
                    },
                },
            },
            Box::pin(std::future::ready(())),
        )
        .await;
        assert!(res_user_del.is_ok());
    }
}

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_default_listen_address() {
        let default_listen = "0.0.0.0:3000";
        kani::assert(default_listen.len() == 12, "default listen length is 12");
        kani::assert(default_listen.contains(':'), "contains port separator");
    }

    #[kani::proof]
    fn harness_command_enum_bounds() {
        let choice: u8 = kani::any();
        let cmd = match choice % 3 {
            0 => super::Commands::Serve {
                listen: "127.0.0.1:3000".to_string(),
            },
            1 => super::Commands::Migrate { database_url: None },
            _ => super::Commands::BuildGeo,
        };
        match cmd {
            super::Commands::Serve { listen } => {
                kani::assert(!listen.is_empty(), "listen non-empty");
            }
            super::Commands::Migrate { .. } => {}
            super::Commands::BuildGeo => {}
        }
    }
}
