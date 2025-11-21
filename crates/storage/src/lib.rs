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
use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

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
        prepare_connection(&db).await?;
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

pub(crate) async fn prepare_connection(db: &DatabaseConnection) -> StorageResult<()> {
    if db.get_database_backend() == DatabaseBackend::Sqlite {
        configure_sqlite(db).await?;
    }

    run_migrations(db).await
}

pub(crate) async fn configure_sqlite(db: &DatabaseConnection) -> StorageResult<()> {
    // WAL mode improves write concurrency; NORMAL keeps durability reasonable
    // without the fsync cost of FULL.
    for pragma in ["PRAGMA journal_mode=WAL;", "PRAGMA synchronous=NORMAL;"] {
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            pragma.to_owned(),
        ))
        .await
        .map_err(StorageError::from_source)?;
    }

    Ok(())
}
