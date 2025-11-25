//! Monitor binary that tails monero-wallet-rpc for qualifying transfers.

use std::io;

use anon_ticket_domain::config::BootstrapConfig;
use anon_ticket_domain::services::telemetry::{init_telemetry, TelemetryConfig};
use anon_ticket_monitor::{build_rpc_source, run_monitor, worker::MonitorError};
use anon_ticket_storage::SeaOrmStorage;

#[tokio::main]
async fn main() -> io::Result<()> {
    if std::env::var("ALLOW_STANDALONE_MONITOR")
        .unwrap_or_default()
        .is_empty()
    {
        eprintln!(
            "[monitor] standalone mode is disabled; set ALLOW_STANDALONE_MONITOR=1 for dev/CI. Production should run the embedded monitor inside the API binary."
        );
        return Err(io::Error::other("standalone monitor disabled"));
    }

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
    let source = build_rpc_source(config.monero_rpc_url())?;
    run_monitor(config, storage, source, None).await
}
