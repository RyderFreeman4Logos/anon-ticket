use std::sync::Arc;

use anon_ticket_domain::model::PaymentId;
use anon_ticket_domain::services::{
    cache::{InMemoryPidCache, PidBloom},
    telemetry::TelemetryGuard,
};
use anon_ticket_storage::SeaOrmStorage;

#[derive(Clone)]
pub struct AppState {
    storage: SeaOrmStorage,
    cache: Arc<InMemoryPidCache>,
    telemetry: TelemetryGuard,
    bloom: Option<Arc<PidBloom>>,
}

impl AppState {
    pub fn new(
        storage: SeaOrmStorage,
        cache: Arc<InMemoryPidCache>,
        telemetry: TelemetryGuard,
        bloom: Option<Arc<PidBloom>>,
    ) -> Self {
        Self {
            storage,
            cache,
            telemetry,
            bloom,
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

    pub fn bloom(&self) -> Option<&PidBloom> {
        self.bloom.as_deref()
    }

    pub fn insert_bloom(&self, pid: &PaymentId) {
        if let Some(bloom) = &self.bloom {
            bloom.insert(pid);
        }
    }
}
