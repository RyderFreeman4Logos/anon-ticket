use std::time::Duration;

use metrics::{counter, gauge, histogram};
use thiserror::Error;
use tokio::time::sleep;
use tracing::warn;

use anon_ticket_domain::{
    config::ConfigError,
    services::telemetry::TelemetryError,
    storage::{MonitorStateStore, StorageError},
};
use anon_ticket_storage::SeaOrmStorage;

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

impl From<reqwest::Error> for MonitorError {
    fn from(value: reqwest::Error) -> Self {
        Self::Rpc(value.to_string())
    }
}

pub async fn run_monitor<S>(
    config: anon_ticket_domain::config::BootstrapConfig,
    storage: SeaOrmStorage,
    source: S,
) -> Result<(), MonitorError>
where
    S: TransferSource,
{
    let mut height = storage
        .last_processed_height()
        .await?
        .unwrap_or(config.monitor_start_height());

    loop {
        match source.fetch_transfers(height).await {
            Ok(transfers) => {
                handle_batch(&storage, &source, transfers, &mut height).await?;
            }
            Err(err) => {
                counter!("monitor_rpc_calls_total", 1, "result" => "error");
                warn!(?err, "rpc fetch failed");
            }
        }
        sleep(Duration::from_secs(5)).await;
    }
}

async fn handle_batch<S>(
    storage: &SeaOrmStorage,
    source: &S,
    transfers: TransfersResponse,
    current_height: &mut u64,
) -> Result<(), MonitorError>
where
    S: TransferSource,
{
    counter!("monitor_rpc_calls_total", 1, "result" => "ok");
    histogram!("monitor_batch_entries", transfers.incoming.len() as f64);

    let mut observed_height: Option<u64> = None;

    for entry in &transfers.incoming {
        if let Some(h) = entry.height {
            let h = h as u64;
            observed_height = Some(observed_height.map_or(h, |current| current.max(h)));
        }
        process_entry(storage, entry).await?;
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
