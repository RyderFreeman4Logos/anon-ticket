//! HTTP API binary entry point placeholder.

use std::process;

use anon_ticket_domain::{
    derive_pid_fingerprint, workspace_ready_message, BootstrapConfig, ConfigError,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("[api] bootstrap failed: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), ConfigError> {
    let config = BootstrapConfig::load_from_env()?;
    println!(
        "[api] {} | bind={} | monitor_height={}",
        workspace_ready_message(),
        config.api_bind_address(),
        config.monitor_start_height()
    );

    let sample = derive_pid_fingerprint("0123456789abcdef0123456789abcdef");
    println!("[api] sample pid fingerprint: {sample}");
    Ok(())
}
