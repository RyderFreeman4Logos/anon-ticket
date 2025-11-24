//! Monitor binary that tails monero-wallet-rpc for qualifying transfers.

mod pipeline;
mod rpc;
mod worker;

use std::io;

use anon_ticket_domain::config::BootstrapConfig;
use anon_ticket_domain::services::telemetry::{init_telemetry, TelemetryConfig};
use anon_ticket_storage::SeaOrmStorage;
use monero_rpc::RpcClientBuilder;

use rpc::RpcTransferSource;
use worker::{run_monitor, MonitorError};

#[tokio::main]
async fn main() -> io::Result<()> {
    if let Err(err) = bootstrap().await {
        eprintln!("[monitor] bootstrap failed: {err}");
        return Err(io::Error::other(err.to_string()));
    }

    Ok(())
}

async fn bootstrap() -> Result<(), MonitorError> {
    let config = BootstrapConfig::load_from_env()?;
    let telemetry_config = TelemetryConfig::from_env("MONITOR");
    init_telemetry(&telemetry_config)?;
    let storage = SeaOrmStorage::connect(config.database_url()).await?;
    let rpc_client = RpcClientBuilder::new()
        .build(config.monero_rpc_url().to_string())
        .map_err(|err| MonitorError::Rpc(err.to_string()))?;
    let wallet = rpc_client.wallet();
    let source = RpcTransferSource::new(wallet);
    run_monitor(config, storage, source).await
}
