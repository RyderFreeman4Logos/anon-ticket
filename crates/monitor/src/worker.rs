use std::time::Duration;

use metrics::{counter, gauge, histogram};
use thiserror::Error;
use tokio::time::sleep;
use tracing::warn;

use anon_ticket_domain::{
    config::ConfigError,
    services::telemetry::TelemetryError,
    storage::{MonitorStateStore, PaymentStore, StorageError},
};

use crate::{
    pipeline::process_entry,
    rpc::{TransferSource, TransfersResponse},
};

#[derive(Debug, Error)]
pub enum MonitorError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("telemetry error: {0}")]
    Telemetry(#[from] TelemetryError),
}

pub async fn run_monitor<S, D>(
    config: anon_ticket_domain::config::BootstrapConfig,
    storage: D,
    source: S,
) -> Result<(), MonitorError>
where
    S: TransferSource,
    D: MonitorStateStore + PaymentStore,
{
    let mut height = storage
        .last_processed_height()
        .await?
        .unwrap_or(config.monitor_start_height());
    let min_payment_amount = config.monitor_min_payment_amount();
    let poll_interval = Duration::from_secs(config.monitor_poll_interval_secs());

    loop {
        match source.fetch_transfers(height).await {
            Ok(transfers) => {
                if let Err(err) = handle_batch(
                    &storage,
                    &source,
                    transfers,
                    &mut height,
                    min_payment_amount,
                )
                .await
                {
                    warn!(?err, "batch processing failed, retrying in next cycle");
                }
            }
            Err(err) => {
                counter!("monitor_rpc_calls_total", 1, "result" => "error");
                warn!(?err, "rpc fetch failed");
            }
        }
        sleep(poll_interval).await;
    }
}

async fn handle_batch<S, D>(
    storage: &D,
    source: &S,
    transfers: TransfersResponse,
    current_height: &mut u64,
    min_payment_amount: i64,
) -> Result<(), MonitorError>
where
    S: TransferSource,
    D: MonitorStateStore + PaymentStore,
{
    counter!("monitor_rpc_calls_total", 1, "result" => "ok");
    histogram!("monitor_batch_entries", transfers.incoming.len() as f64);

    let mut observed_height: Option<u64> = None;

    for entry in &transfers.incoming {
        if let Some(h) = entry.height {
            let h = h as u64;
            observed_height = Some(observed_height.map_or(h, |current| current.max(h)));
        }
        process_entry(storage, entry, min_payment_amount).await?;
    }

    let mut next_height = *current_height;
    if let Some(max_height) = observed_height {
        next_height = max_height + 1;
    } else if let Ok(chain_height) = source.wallet_height().await {
        next_height = chain_height.max(next_height);
    }

    storage.upsert_last_processed_height(next_height).await?;
    gauge!("monitor_last_height", next_height as f64);
    *current_height = next_height;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anon_ticket_domain::model::{ClaimOutcome, NewPayment, PaymentId, PaymentRecord};
    use anon_ticket_domain::storage::{PaymentStore, StorageResult};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct MockStorage {
        should_fail: Arc<AtomicBool>,
    }

    #[async_trait]
    impl MonitorStateStore for MockStorage {
        async fn last_processed_height(&self) -> StorageResult<Option<u64>> {
            Ok(Some(100))
        }
        async fn upsert_last_processed_height(&self, _height: u64) -> StorageResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl PaymentStore for MockStorage {
        async fn insert_payment(&self, _payment: NewPayment) -> StorageResult<()> {
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(StorageError::Database("simulated failure".into()));
            }
            Ok(())
        }
        async fn claim_payment(&self, _pid: &PaymentId) -> StorageResult<Option<ClaimOutcome>> {
            Ok(None)
        }
        async fn find_payment(&self, _pid: &PaymentId) -> StorageResult<Option<PaymentRecord>> {
            Ok(None)
        }
    }

    struct MockSource;

    #[async_trait]
    impl TransferSource for MockSource {
        async fn fetch_transfers(
            &self,
            _start_height: u64,
        ) -> Result<TransfersResponse, MonitorError> {
            Ok(TransfersResponse { incoming: vec![] })
        }
        async fn wallet_height(&self) -> Result<u64, MonitorError> {
            Ok(100)
        }
    }

    #[tokio::test]
    async fn handle_batch_propagates_storage_error() {
        let should_fail = Arc::new(AtomicBool::new(true));
        let storage = MockStorage {
            should_fail: should_fail.clone(),
        };
        let source = MockSource;
        let mut height = 100;

        let transfers = TransfersResponse {
            incoming: vec![crate::rpc::TransferEntry {
                txid: "tx1".into(),
                payment_id: Some("1111111111111111".into()),
                amount: 100,
                height: Some(101),
                timestamp: 0,
            }],
        };

        // Should fail
        let result = handle_batch(&storage, &source, transfers.clone(), &mut height, 1).await;
        assert!(result.is_err());

        // Should succeed
        should_fail.store(false, Ordering::SeqCst);
        let result = handle_batch(&storage, &source, transfers, &mut height, 1).await;
        assert!(result.is_ok());
    }
}
