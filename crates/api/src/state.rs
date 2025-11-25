use std::sync::Arc;

use anon_ticket_domain::services::{cache::InMemoryPidCache, telemetry::TelemetryGuard};
use anon_ticket_storage::SeaOrmStorage;
use std::time::Duration;

#[derive(Clone)]
pub struct AppState {
    storage: SeaOrmStorage,
    cache: Arc<InMemoryPidCache>,
    telemetry: TelemetryGuard,
    negative_grace: Duration,
}

impl AppState {
    pub fn new(
        storage: SeaOrmStorage,
        cache: Arc<InMemoryPidCache>,
        telemetry: TelemetryGuard,
        negative_grace: Duration,
    ) -> Self {
        Self {
            storage,
            cache,
            telemetry,
            negative_grace,
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

    pub fn negative_grace(&self) -> Duration {
        self.negative_grace
    }
}
