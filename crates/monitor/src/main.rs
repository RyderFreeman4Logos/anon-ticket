//! Monitor binary that tails monero-wallet-rpc for qualifying transfers.

mod client;

use std::time::Duration;

use anon_ticket_domain::{
    init_telemetry,
    storage::{MonitorStateStore, NewPayment, PaymentId, PaymentStore},
    validate_pid, BootstrapConfig, ConfigError, TelemetryConfig, TelemetryError,
};
use anon_ticket_storage::SeaOrmStorage;
use chrono::{DateTime, Utc};
use client::{JsonRpcRequest, JsonRpcResponse, TransferEntry, TransfersResponse};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use thiserror::Error;
use tokio::time::sleep;
use tracing::warn;

#[derive(Debug, Error)]
pub enum MonitorError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("storage error: {0}")]
    Storage(#[from] anon_ticket_domain::storage::StorageError),
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

#[derive(Clone)]
struct MonitorCtx {
    config: BootstrapConfig,
    storage: SeaOrmStorage,
    client: Client,
}

#[tokio::main]
async fn main() -> Result<(), MonitorError> {
    dotenvy::dotenv().ok();
    let config = BootstrapConfig::load_from_env()?;
    let telemetry_config = TelemetryConfig::from_env("MONITOR");
    let _telemetry = init_telemetry(&telemetry_config)?;
    let storage = SeaOrmStorage::connect(config.database_url()).await?;
    let client = Client::builder().build()?;
    run(MonitorCtx {
        config,
        storage,
        client,
    })
    .await
}

async fn run(ctx: MonitorCtx) -> Result<(), MonitorError> {
    let mut height = ctx
        .storage
        .last_processed_height()
        .await?
        .unwrap_or(ctx.config.monitor_start_height());

    loop {
        match fetch_transfers(&ctx, height).await {
            Ok(transfers) => {
                metrics::counter!("monitor_rpc_calls_total", 1, "result" => "ok");
                let mut max_height = height;
                let mut advanced = false;
                metrics::histogram!("monitor_batch_entries", transfers.incoming.len() as f64);

                for entry in &transfers.incoming {
                    if let Some(h) = entry.height {
                        max_height = max_height.max(h as u64);
                    }
                    if process_entry(&ctx, entry).await? {
                        advanced = true;
                    }
                }

                if advanced {
                    height = max_height + 1;
                    ctx.storage.upsert_last_processed_height(height).await?;
                    metrics::gauge!("monitor_last_height", height as f64);
                }
            }
            Err(err) => {
                metrics::counter!("monitor_rpc_calls_total", 1, "result" => "error");
                warn!(?err, "rpc fetch failed");
            }
        }
        sleep(Duration::from_secs(5)).await;
    }
}

async fn process_entry(ctx: &MonitorCtx, entry: &TransferEntry) -> Result<bool, MonitorError> {
    let (Some(pid), Some(height)) = (&entry.payment_id, entry.height) else {
        return Ok(false);
    };

    if validate_pid(pid).is_err() {
        warn!(pid, "skipping invalid pid");
        metrics::counter!("monitor_payments_ingested_total", 1, "result" => "invalid_pid");
        return Ok(false);
    }

    let detected_at = DateTime::from_timestamp(entry.timestamp as i64, 0).unwrap_or_else(Utc::now);

    ctx.storage
        .insert_payment(NewPayment {
            pid: PaymentId::new(pid.clone()),
            txid: entry.txid.clone(),
            amount: entry.amount,
            block_height: height,
            detected_at,
        })
        .await?;
    metrics::counter!("monitor_payments_ingested_total", 1, "result" => "persisted");

    Ok(true)
}

async fn fetch_transfers(
    ctx: &MonitorCtx,
    start_height: u64,
) -> Result<TransfersResponse, MonitorError> {
    #[derive(Serialize)]
    struct Params {
        #[serde(rename = "in")]
        in_transfers: bool,
        out: bool,
        pending: bool,
        filter_by_height: bool,
        min_height: u64,
    }

    let params = Params {
        in_transfers: true,
        out: false,
        pending: false,
        filter_by_height: true,
        min_height: start_height,
    };

    let request = JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "get_transfers".into(),
        params,
    };

    let mut builder = ctx.client.post(ctx.config.monero_rpc_url());
    builder = builder.basic_auth(
        ctx.config.monero_rpc_user(),
        Some(ctx.config.monero_rpc_pass()),
    );

    let resp = builder.json(&request).send().await?;
    if resp.status() != StatusCode::OK {
        return Err(MonitorError::Rpc(format!("rpc failure {}", resp.status())));
    }

    let parsed: JsonRpcResponse<TransfersResponse> = resp.json().await?;
    Ok(parsed.result)
}
