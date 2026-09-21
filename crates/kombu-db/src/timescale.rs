#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use sqlx::PgPool;

pub async fn is_timescale_installed(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let row = sqlx::query_as::<_, (bool,)>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM pg_extension WHERE extname = 'timescaledb'
        )
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn register_hypertable(
    pool: &PgPool,
    table_name: &str,
    time_column: &str,
    chunk_interval: &str,
    compression: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO "timescale_hypertable_registry" (
            hypertable_name, time_column, chunk_interval, compression_enabled
        ) VALUES ($1, $2, $3, $4)
        ON CONFLICT (hypertable_name) DO UPDATE SET
            time_column = EXCLUDED.time_column,
            chunk_interval = EXCLUDED.chunk_interval,
            compression_enabled = EXCLUDED.compression_enabled
        "#,
    )
    .bind(table_name)
    .bind(time_column)
    .bind(chunk_interval)
    .bind(compression)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_registered_hypertables(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT hypertable_name
        FROM "timescale_hypertable_registry"
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_timescale_registry() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let installed = is_timescale_installed(&pool).await.unwrap();
        let _ = installed;

        let reg = register_hypertable(&pool, "website_event", "created_at", "7 days", false).await;
        assert!(reg.is_ok());

        let list = get_registered_hypertables(&pool).await.unwrap();
        assert!(list.contains(&"website_event".to_string()));
    }

    #[tokio::test]
    async fn test_timescale_error_paths() {
        let pool =
            sqlx::PgPool::connect_lazy("postgres://kombu:kombu@localhost:5432/kombu").unwrap();
        pool.close().await;

        assert!(is_timescale_installed(&pool).await.is_err());
        assert!(
            register_hypertable(&pool, "website_event", "created_at", "7 days", false)
                .await
                .is_err()
        );
        assert!(get_registered_hypertables(&pool).await.is_err());
    }
}
