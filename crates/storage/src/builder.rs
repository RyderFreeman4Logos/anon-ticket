use anon_ticket_domain::storage::StorageResult;
use sea_orm::Database;

use crate::{errors::StorageError, prepare_connection, SeaOrmStorage};

#[derive(Default)]
pub struct StorageBuilder {
    database_url: Option<String>,
}

impl StorageBuilder {
    pub fn new() -> Self {
        Self { database_url: None }
    }

    pub fn database_url(mut self, url: impl Into<String>) -> Self {
        self.database_url = Some(url.into());
        self
    }

    pub async fn build(self) -> StorageResult<SeaOrmStorage> {
        let url = self
            .database_url
            .ok_or_else(|| StorageError::Database("missing database url".into()))?;
        let db = Database::connect(url)
            .await
            .map_err(StorageError::from_source)?;
        prepare_connection(&db).await?;
        Ok(SeaOrmStorage::from_connection(db))
    }
}
