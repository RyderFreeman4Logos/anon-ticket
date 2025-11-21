use std::sync::Arc;

use anon_ticket_domain::services::{cache::InMemoryPidCache, telemetry::TelemetryGuard};
use anon_ticket_storage::SeaOrmStorage;

#[derive(Clone)]
pub struct AppState {
    storage: SeaOrmStorage,
    cache: Arc<InMemoryPidCache>,
    telemetry: TelemetryGuard,
}

impl AppState {
    pub fn new(
        storage: SeaOrmStorage,
        cache: Arc<InMemoryPidCache>,
        telemetry: TelemetryGuard,
    ) -> Self {
        Self {
            storage,
            cache,
            telemetry,
        }
    }

    pub fn storage(&self) -> &SeaOrmStorage {
        &self.storage
    }

    pub fn cache(&self) -> &InMemoryPidCache {
        self.cache.as_ref()
    }

    pub fn telemetry(&self) -> &TelemetryGuard {
        &self.telemetry
    }
}
