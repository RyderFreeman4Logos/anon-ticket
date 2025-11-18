//! SeaORM-backed storage adapters that satisfy the domain storage traits while
//! keeping the database backend swappable (SQLite by default, PostgreSQL via
//! feature flag).

mod builder;
mod entity;
mod errors;
mod migration;
mod monitor_state_store;
mod payment_store;
mod token_store;

use std::sync::Arc;

use anon_ticket_domain::storage::StorageResult;
use builder::StorageBuilder;
use errors::StorageError;
use migration::run_migrations;
use sea_orm::{Database, DatabaseConnection};

/// Shared storage handle used by the HTTP API and monitor services.
#[derive(Clone)]
pub struct SeaOrmStorage {
    db: Arc<DatabaseConnection>,
}

impl SeaOrmStorage {
    /// Connects to the provided database URL and ensures the schema is present.
    pub async fn connect(database_url: &str) -> StorageResult<Self> {
        let db = Database::connect(database_url)
            .await
            .map_err(StorageError::from_source)?;
        run_migrations(&db).await?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn builder() -> StorageBuilder {
        StorageBuilder::new()
    }

    pub(crate) fn from_connection(db: DatabaseConnection) -> Self {
        Self { db: Arc::new(db) }
    }

    pub fn connection(&self) -> &DatabaseConnection {
        self.db.as_ref()
    }
}
