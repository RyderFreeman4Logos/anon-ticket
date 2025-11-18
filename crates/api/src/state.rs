use std::sync::Arc;

use anon_ticket_domain::services::{
    cache::InMemoryPidCache,
    telemetry::{AbuseTracker, TelemetryGuard},
};
use anon_ticket_storage::SeaOrmStorage;

#[derive(Clone)]
pub struct AppState {
    storage: SeaOrmStorage,
    cache: Arc<InMemoryPidCache>,
    telemetry: TelemetryGuard,
    abuse_tracker: AbuseTracker,
}

impl AppState {
    pub fn new(
        storage: SeaOrmStorage,
        cache: Arc<InMemoryPidCache>,
        telemetry: TelemetryGuard,
        abuse_tracker: AbuseTracker,
    ) -> Self {
        Self {
            storage,
            cache,
            telemetry,
            abuse_tracker,
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

    pub fn abuse_tracker(&self) -> &AbuseTracker {
        &self.abuse_tracker
    }
}
