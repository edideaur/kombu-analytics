#![forbid(unsafe_code)]
use anyhow::{Context, bail};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
pub struct WebsiteRow {
    pub website_id: Uuid,
    pub name: String,
    pub domain: Option<String>,
    pub created_at: Option<chrono::DateTime<Utc>>,
}

pub async fn list_websites(pool: &PgPool) -> anyhow::Result<Vec<WebsiteRow>> {
    let rows = sqlx::query_as::<_, WebsiteRow>(
        r#"SELECT website_id, name, domain, created_at FROM "website" WHERE deleted_at IS NULL ORDER BY created_at ASC"#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to query websites")?;
    Ok(rows)
}

pub async fn create_website(
    pool: &PgPool,
    name: &str,
    domain: Option<&str>,
    user_id: Option<Uuid>,
) -> anyhow::Result<Uuid> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Website name cannot be empty");
    }

    let owner_id = if let Some(uid) = user_id {
        uid
    } else {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT user_id FROM "user" WHERE deleted_at IS NULL ORDER BY created_at ASC LIMIT 1"#,
        )
        .fetch_optional(pool)
        .await
        .context("Failed to find default user")?
        .context("No users found in database to assign as website owner. Create a user first.")?
    };

    let website_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO "website" (website_id, name, domain, user_id, created_by, created_at) VALUES ($1, $2, $3, $4, $4, NOW())"#,
    )
    .bind(website_id)
    .bind(name)
    .bind(domain.map(str::trim))
    .bind(owner_id)
    .execute(pool)
    .await
    .context("Failed to insert website")?;

    Ok(website_id)
}

pub async fn delete_website(pool: &PgPool, website_id: Uuid) -> anyhow::Result<()> {
    let res = sqlx::query(
        r#"UPDATE "website" SET deleted_at = NOW() WHERE website_id = $1 AND deleted_at IS NULL"#,
    )
    .bind(website_id)
    .execute(pool)
    .await
    .context("Failed to delete website")?;

    if res.rows_affected() == 0 {
        bail!("Website with ID {website_id} not found");
    }

    Ok(())
}

pub async fn reset_website(pool: &PgPool, website_id: Uuid) -> anyhow::Result<()> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "website" WHERE website_id = $1 AND deleted_at IS NULL)"#,
    )
    .bind(website_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if !exists {
        bail!("Website with ID {website_id} not found");
    }

    let mut tx = pool.begin().await?;

    sqlx::query(r#"DELETE FROM "website_event" WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&mut *tx)
        .await
        .context("Failed to clear website events")?;

    sqlx::query(r#"DELETE FROM "session" WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&mut *tx)
        .await
        .context("Failed to clear website sessions")?;

    sqlx::query(r#"UPDATE "website" SET reset_at = NOW() WHERE website_id = $1"#)
        .bind(website_id)
        .execute(&mut *tx)
        .await
        .context("Failed to update website reset_at timestamp")?;

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_website_validations() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        if let Ok(pool) = PgPool::connect(&db_url).await {
            assert!(
                create_website(&pool, "", Some("example.com"), None)
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn test_website_lifecycle() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        if let Ok(pool) = PgPool::connect(&db_url).await {
            let site_name = format!("Site_{}", Uuid::now_v7());
            let site_id = create_website(&pool, &site_name, Some("example.org"), None)
                .await
                .expect("create site ok");

            let sites = list_websites(&pool).await.expect("list sites ok");
            assert!(sites.iter().any(|s| s.website_id == site_id));

            reset_website(&pool, site_id).await.expect("reset site ok");
            delete_website(&pool, site_id)
                .await
                .expect("delete site ok");
            assert!(delete_website(&pool, site_id).await.is_err());
            assert!(reset_website(&pool, site_id).await.is_err());
        }
    }
}
