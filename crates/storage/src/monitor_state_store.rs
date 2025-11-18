use anon_ticket_domain::storage::{MonitorStateStore, StorageResult};
use sea_orm::{sea_query::OnConflict, EntityTrait, Set};

use crate::entity::monitor_state;
use crate::errors::StorageError;
use crate::SeaOrmStorage;

const LAST_HEIGHT_KEY: &str = "last_processed_height";

#[async_trait::async_trait]
impl MonitorStateStore for SeaOrmStorage {
    async fn last_processed_height(&self) -> StorageResult<Option<u64>> {
        let maybe = monitor_state::Entity::find_by_id(LAST_HEIGHT_KEY.to_string())
            .one(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(maybe.map(|model| model.value_int as u64))
    }

    async fn upsert_last_processed_height(&self, height: u64) -> StorageResult<()> {
        let active = monitor_state::ActiveModel {
            key: Set(LAST_HEIGHT_KEY.to_string()),
            value_int: Set(height as i64),
        };
        monitor_state::Entity::insert(active)
            .on_conflict(
                OnConflict::column(monitor_state::Column::Key)
                    .update_column(monitor_state::Column::ValueInt)
                    .to_owned(),
            )
            .exec(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(())
    }
}
