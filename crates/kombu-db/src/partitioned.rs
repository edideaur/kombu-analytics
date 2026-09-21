#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn refresh_hourly_rollups(
    pool: &PgPool,
    website_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT refresh_website_event_stats_hourly($1, $2, $3)")
        .bind(website_id)
        .bind(start_at)
        .bind(end_at)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn register_partition(
    pool: &PgPool,
    table_name: &str,
    parent_table: &str,
    partition_key: &str,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO "analytics_partition_registry" (
            table_name, parent_table, partition_key, range_start, range_end
        ) VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (table_name) DO UPDATE SET
            range_start = EXCLUDED.range_start,
            range_end = EXCLUDED.range_end
        "#,
    )
    .bind(table_name)
    .bind(parent_table)
    .bind(partition_key)
    .bind(range_start)
    .bind(range_end)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_registered_partitions(
    pool: &PgPool,
    parent_table: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT table_name
        FROM "analytics_partition_registry"
        WHERE parent_table = $1
        ORDER BY range_start ASC
        "#,
    )
    .bind(parent_table)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.0).collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_partitioned_registry_and_rollups() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        let pool = sqlx::PgPool::connect(&db_url).await.unwrap();
        let website_id = Uuid::now_v7();
        let start = Utc::now() - chrono::Duration::hours(2);
        let end = Utc::now();

        let refresh_res = refresh_hourly_rollups(&pool, website_id, start, end).await;
        assert!(refresh_res.is_ok());

        let table_name = format!("website_event_test_{}", &website_id.to_string()[..8]);
        let reg_res = register_partition(
            &pool,
            &table_name,
            "website_event",
            "created_at",
            start,
            end,
        )
        .await;
        assert!(reg_res.is_ok());

        let parts = get_registered_partitions(&pool, "website_event").await.unwrap();
        assert!(parts.contains(&table_name));
    }

    #[tokio::test]
    async fn test_partitioned_error_paths() {
        let pool =
            sqlx::PgPool::connect_lazy("postgres://kombu:kombu@localhost:5432/kombu").unwrap();
        pool.close().await;
        let website_id = Uuid::now_v7();
        let start = Utc::now() - chrono::Duration::hours(2);
        let end = Utc::now();

        assert!(refresh_hourly_rollups(&pool, website_id, start, end).await.is_err());
        assert!(
            register_partition(&pool, "t", "website_event", "created_at", start, end)
                .await
                .is_err()
        );
        assert!(get_registered_partitions(&pool, "website_event").await.is_err());
    }
}
