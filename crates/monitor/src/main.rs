//! Monitor binary entry point placeholder.

use std::process;

use anon_ticket_domain::{derive_pid_fingerprint, BootstrapConfig, ConfigError};

fn main() {
    if let Err(err) = run() {
        eprintln!("[monitor] bootstrap failed: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), ConfigError> {
    let config = BootstrapConfig::load_from_env()?;
    println!(
        "[monitor] polling {} starting at height {}",
        config.monero_rpc_url(),
        config.monitor_start_height()
    );

    let health_token = derive_pid_fingerprint(config.monero_rpc_user());
    println!("[monitor] rpc credentials fingerprint: {health_token}");
    Ok(())
}
