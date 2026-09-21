#![forbid(unsafe_code)]
use arc_swap::ArcSwap;
use sqlx::PgPool;
use std::sync::Arc;

pub use kombu_core::types::StorageEngine;

pub mod clickhouse;
pub mod partitioned;
pub mod timescale;

pub use clickhouse::{ClickHouseClient, ClickHouseConfig, ClickHouseError, ClickHouseEvent};

#[derive(Clone)]
pub struct Db {
    pool: Arc<ArcSwap<PgPool>>,
}

impl Db {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool: Arc::new(ArcSwap::from_pointee(pool)),
        }
    }
    pub fn pool(&self) -> Arc<PgPool> {
        self.pool.load_full()
    }
    pub fn set_pool(&self, pool: PgPool) {
        self.pool.store(Arc::new(pool));
    }
}

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(database_url).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::manual_let_else)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_pool_lifecycle() {
        let pool = create_pool("postgres://kombu:kombu@localhost:5432/kombu")
            .await
            .unwrap();

        let db = Db::new(pool.clone());
        let _ = db.clone();
        assert_eq!(db.pool().size(), pool.size());

        db.set_pool(pool.clone());
        assert_eq!(db.pool().size(), pool.size());
    }
}
