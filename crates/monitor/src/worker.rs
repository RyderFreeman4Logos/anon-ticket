use std::time::Duration;

use metrics::{counter, gauge, histogram};
use thiserror::Error;
use tokio::time::sleep;
use tracing::warn;

use anon_ticket_domain::{
    config::ConfigError,
    services::{
        cache::{PidBloom, PidCache},
        telemetry::TelemetryError,
    },
    storage::{MonitorStateStore, PaymentStore, StorageError},
    PaymentId,
};
use monero_rpc::RpcClientBuilder;

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
    hooks: Option<MonitorHooks>,
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
    let min_confirmations = config.monitor_min_confirmations();
    let poll_interval = Duration::from_secs(config.monitor_poll_interval_secs());

    loop {
        let wallet_height = match source.wallet_height().await {
            Ok(height) => height,
            Err(err) => {
                warn!(?err, "rpc height fetch failed");
                sleep(poll_interval).await;
                continue;
            }
        };

        let safe_height = wallet_height
            .saturating_add(1)
            .saturating_sub(min_confirmations);

        if height > safe_height {
            // wait for more confirmations before progressing
            sleep(poll_interval).await;
            continue;
        }

        match monitor_tick(
            &storage,
            &source,
            &mut height,
            min_payment_amount,
            safe_height,
            hooks.as_ref(),
        )
        .await
        {
            Ok(()) => {}
            Err(err) => warn!(?err, "batch processing failed, retrying in next cycle"),
        }
        sleep(poll_interval).await;
    }
}

async fn monitor_tick<S, D>(
    storage: &D,
    source: &S,
    current_height: &mut u64,
    min_payment_amount: i64,
    safe_height: u64,
    hooks: Option<&MonitorHooks>,
) -> Result<(), MonitorError>
where
    S: TransferSource,
    D: MonitorStateStore + PaymentStore,
{
    if *current_height > safe_height {
        return Ok(());
    }

    let transfers = match source.fetch_transfers(*current_height, safe_height).await {
        Ok(resp) => resp,
        Err(err) => {
            counter!("monitor_rpc_calls_total", 1, "result" => "error");
            return Err(err);
        }
    };

    handle_batch(
        storage,
        transfers,
        current_height,
        min_payment_amount,
        safe_height,
        hooks,
    )
    .await
}

async fn handle_batch<D>(
    storage: &D,
    transfers: TransfersResponse,
    current_height: &mut u64,
    min_payment_amount: i64,
    safe_height: u64,
    hooks: Option<&MonitorHooks>,
) -> Result<(), MonitorError>
where
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
        process_entry(storage, entry, min_payment_amount, hooks).await?;
    }

    let mut next_height = if let Some(max_height) = observed_height {
        max_height.saturating_add(1)
    } else {
        safe_height.saturating_add(1)
    };
    next_height = next_height.min(safe_height.saturating_add(1));

    storage.upsert_last_processed_height(next_height).await?;
    gauge!("monitor_last_height", next_height as f64);
    *current_height = next_height;
    Ok(())
}

#[derive(Clone)]
pub struct MonitorHooks {
    pid_cache: Option<std::sync::Arc<dyn PidCache>>, // marks present after persistence
    pid_bloom: Option<std::sync::Arc<PidBloom>>,     // inserts after persistence
}

impl MonitorHooks {
    pub fn new(
        pid_cache: Option<std::sync::Arc<dyn PidCache>>,
        pid_bloom: Option<std::sync::Arc<PidBloom>>,
    ) -> Self {
        Self {
            pid_cache,
            pid_bloom,
        }
    }

    pub fn mark_present(&self, pid: &PaymentId) {
        if let Some(cache) = &self.pid_cache {
            cache.mark_present(pid);
        }
        if let Some(bloom) = &self.pid_bloom {
            bloom.insert(pid);
        }
    }
}

pub fn build_rpc_source(url: &str) -> Result<crate::rpc::RpcTransferSource, MonitorError> {
    let normalized = url.strip_suffix("/json_rpc").unwrap_or(url);
    let rpc_client = RpcClientBuilder::new()
        .build(normalized.to_string())
        .map_err(|err| MonitorError::Rpc(err.to_string()))?;
    Ok(crate::rpc::RpcTransferSource::new(rpc_client.wallet()))
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

    #[tokio::test]
    async fn handle_batch_propagates_storage_error() {
        let should_fail = Arc::new(AtomicBool::new(true));
        let storage = MockStorage {
            should_fail: should_fail.clone(),
        };
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
        let result = handle_batch(&storage, transfers.clone(), &mut height, 1, 200, None).await;
        assert!(result.is_err());

        // Should succeed
        should_fail.store(false, Ordering::SeqCst);
        let result = handle_batch(&storage, transfers, &mut height, 1, 200, None).await;
        assert!(result.is_ok());
    }

    #[derive(Clone)]
    struct RecordingSource {
        fetch_called: Arc<AtomicBool>,
    }

    #[async_trait]
    impl TransferSource for RecordingSource {
        async fn fetch_transfers(
            &self,
            _start_height: u64,
            _max_height: u64,
        ) -> Result<TransfersResponse, MonitorError> {
            self.fetch_called.store(true, Ordering::SeqCst);
            Ok(TransfersResponse { incoming: vec![] })
        }

        async fn wallet_height(&self) -> Result<u64, MonitorError> {
            Ok(50)
        }
    }

    #[tokio::test]
    async fn monitor_skips_when_height_above_safe_window() {
        let storage = MockStorage {
            should_fail: Arc::new(AtomicBool::new(false)),
        };
        let source = RecordingSource {
            fetch_called: Arc::new(AtomicBool::new(false)),
        };
        let mut height = 60;
        let safe_height = 40;

        monitor_tick(&storage, &source, &mut height, 1, safe_height, None)
            .await
            .expect("tick succeeds");

        // Should not call fetch because current height is beyond the safe window.
        assert!(!source.fetch_called.load(Ordering::SeqCst));
        // Cursor should remain unchanged.
        assert_eq!(height, 60);
    }

    #[derive(Clone)]
    struct PreparedSource {
        transfers: Arc<Vec<crate::rpc::TransferEntry>>,
    }

    #[async_trait]
    impl TransferSource for PreparedSource {
        async fn fetch_transfers(
            &self,
            _start_height: u64,
            _max_height: u64,
        ) -> Result<TransfersResponse, MonitorError> {
            Ok(TransfersResponse {
                incoming: self.transfers.as_ref().clone(),
            })
        }

        async fn wallet_height(&self) -> Result<u64, MonitorError> {
            Ok(120)
        }
    }

    #[tokio::test]
    async fn monitor_advances_only_to_safe_height() {
        let storage = MockStorage {
            should_fail: Arc::new(AtomicBool::new(false)),
        };
        let transfers = vec![crate::rpc::TransferEntry {
            txid: "tx1".into(),
            payment_id: Some("1111111111111111".into()),
            amount: 100,
            height: Some(115),
            timestamp: 0,
        }];
        let source = PreparedSource {
            transfers: Arc::new(transfers),
        };
        let mut height = 110;
        let safe_height = 115;

        monitor_tick(&storage, &source, &mut height, 1, safe_height, None)
            .await
            .expect("tick succeeds");

        assert_eq!(height, safe_height.saturating_add(1));
    }
}
