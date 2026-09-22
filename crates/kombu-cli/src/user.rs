#![forbid(unsafe_code)]
use anyhow::{Context, bail};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow, PartialEq, Eq)]
pub struct UserRow {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub created_at: Option<chrono::DateTime<Utc>>,
}

pub async fn list_users(pool: &PgPool) -> anyhow::Result<Vec<UserRow>> {
    let rows = sqlx::query_as::<_, UserRow>(
        r#"SELECT user_id, username, role, created_at FROM "user" WHERE deleted_at IS NULL ORDER BY created_at ASC"#
    )
    .fetch_all(pool)
    .await
    .context("Failed to query users")?;
    Ok(rows)
}

pub async fn create_user(
    pool: &PgPool,
    username: &str,
    password: &str,
    role: &str,
) -> anyhow::Result<Uuid> {
    let username = username.trim();
    if username.is_empty() {
        bail!("Username cannot be empty");
    }
    if password.len() < 8 {
        bail!("Password must be at least 8 characters long");
    }
    let role = role.trim().to_lowercase();
    if role != "admin" && role != "user" && role != "view-only" {
        bail!("Role must be one of: 'admin', 'user', 'view-only'");
    }

    let exists = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "user" WHERE username = $1 AND deleted_at IS NULL)"#,
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if exists {
        bail!("User with username '{username}' already exists");
    }

    let hashed = bcrypt::hash(password, 10).context("Failed to hash password")?;
    let user_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO "user" (user_id, username, password, role, created_at) VALUES ($1, $2, $3, $4, NOW())"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(&hashed)
    .bind(&role)
    .execute(pool)
    .await
    .context("Failed to insert user")?;

    Ok(user_id)
}

pub async fn reset_password(
    pool: &PgPool,
    username: &str,
    new_password: &str,
) -> anyhow::Result<()> {
    let username = username.trim();
    if new_password.len() < 8 {
        bail!("Password must be at least 8 characters long");
    }

    let hashed = bcrypt::hash(new_password, 10).context("Failed to hash password")?;

    let res = sqlx::query(
        r#"UPDATE "user" SET password = $1, updated_at = NOW() WHERE username = $2 AND deleted_at IS NULL"#,
    )
    .bind(&hashed)
    .bind(username)
    .execute(pool)
    .await
    .context("Failed to update user password")?;

    if res.rows_affected() == 0 {
        bail!("User '{username}' not found");
    }

    Ok(())
}

pub async fn delete_user(pool: &PgPool, username: &str) -> anyhow::Result<()> {
    let username = username.trim();
    let res = sqlx::query(
        r#"UPDATE "user" SET deleted_at = NOW() WHERE username = $1 AND deleted_at IS NULL"#,
    )
    .bind(username)
    .execute(pool)
    .await
    .context("Failed to delete user")?;

    if res.rows_affected() == 0 {
        bail!("User '{username}' not found");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_user_validations() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        if let Ok(pool) = PgPool::connect(&db_url).await {
            assert!(
                create_user(&pool, "", "ValidPass123!", "admin")
                    .await
                    .is_err()
            );
            assert!(
                create_user(&pool, "validuser", "short", "admin")
                    .await
                    .is_err()
            );
            assert!(
                create_user(&pool, "validuser", "ValidPass123!", "invalid_role")
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn test_user_lifecycle() {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://kombu:kombu@localhost:5432/kombu".to_string());
        if let Ok(pool) = PgPool::connect(&db_url).await {
            let uname = format!("cli_test_{}", Uuid::now_v7());
            let uid = create_user(&pool, &uname, "StrongP@ss123", "admin")
                .await
                .expect("user create ok");

            let users = list_users(&pool).await.expect("list users ok");
            assert!(
                users
                    .iter()
                    .any(|u| u.user_id == uid && u.username == uname)
            );

            // Test duplicate username rejection
            assert!(
                create_user(&pool, &uname, "StrongP@ss123", "user")
                    .await
                    .is_err()
            );

            // Test reset password
            assert!(reset_password(&pool, &uname, "short").await.is_err());
            reset_password(&pool, &uname, "NewStrongP@ss456")
                .await
                .expect("reset pass ok");

            // Test delete user
            delete_user(&pool, &uname).await.expect("delete user ok");
            assert!(delete_user(&pool, &uname).await.is_err());
            assert!(
                reset_password(&pool, &uname, "NewStrongP@ss456")
                    .await
                    .is_err()
            );
        }
    }
}
