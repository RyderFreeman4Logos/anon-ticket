//! Domain-level building blocks shared across API and monitor crates.
//!
//! The current placeholder focuses on proving that the multi-crate workspace
//! is wired up correctly while already enforcing deterministic configuration
//! loading and hashing helpers shared between binaries.

use std::env;

use hex::encode as hex_encode;
use sha3::{Digest, Sha3_256};
use thiserror::Error;

/// Key configuration derived from `.env`/process variables so binaries can
/// share a deterministic environment contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapConfig {
    database_url: String,
    api_bind_address: String,
    monero_rpc_url: String,
    monero_rpc_user: String,
    monero_rpc_pass: String,
    monitor_start_height: u64,
}

impl BootstrapConfig {
    /// Loads configuration by hydrating `.env` (if present) and reading the
    /// required process variables. Missing or malformed entries surface as
    /// `ConfigError` so binaries can respond gracefully.
    pub fn load_from_env() -> Result<Self, ConfigError> {
        match dotenvy::dotenv() {
            Ok(_) => {}
            Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(ConfigError::Dotenv { source: err }),
        }

        let database_url = get_required_var("DATABASE_URL")?;
        let api_bind_address = get_required_var("API_BIND_ADDRESS")?;
        let monero_rpc_url = get_required_var("MONERO_RPC_URL")?;
        let monero_rpc_user = get_required_var("MONERO_RPC_USER")?;
        let monero_rpc_pass = get_required_var("MONERO_RPC_PASS")?;
        let monitor_start_height =
            get_required_var("MONITOR_START_HEIGHT")?
                .parse()
                .map_err(|source| ConfigError::InvalidNumber {
                    key: "MONITOR_START_HEIGHT",
                    source,
                })?;

        Ok(Self {
            database_url,
            api_bind_address,
            monero_rpc_url,
            monero_rpc_user,
            monero_rpc_pass,
            monitor_start_height,
        })
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn api_bind_address(&self) -> &str {
        &self.api_bind_address
    }

    pub fn monero_rpc_url(&self) -> &str {
        &self.monero_rpc_url
    }

    pub fn monero_rpc_user(&self) -> &str {
        &self.monero_rpc_user
    }

    pub fn monero_rpc_pass(&self) -> &str {
        &self.monero_rpc_pass
    }

    pub fn monitor_start_height(&self) -> u64 {
        self.monitor_start_height
    }
}

fn get_required_var(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::MissingVar { key })
}

/// Errors emitted when `.env` hydration or environment parsing fails.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable `{key}`")]
    MissingVar { key: &'static str },
    #[error("invalid integer in `{key}`: {source}")]
    InvalidNumber {
        key: &'static str,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("failed to load .env file: {source}")]
    Dotenv {
        #[from]
        source: dotenvy::Error,
    },
}

/// Returns a static readiness message shared by sibling crates.
pub fn workspace_ready_message() -> &'static str {
    "anon-ticket workspace scaffolding ready"
}

/// Deterministically derives a SHA3-256 fingerprint for a PID or token seed.
/// This keeps hashing consistent across binaries until the full token module
/// lands.
pub fn derive_pid_fingerprint(pid: &str) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(pid.as_bytes());
    let digest = hasher.finalize();
    hex_encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn set_env() {
        env::set_var("DATABASE_URL", "sqlite://test.db");
        env::set_var("API_BIND_ADDRESS", "127.0.0.1:8080");
        env::set_var("MONERO_RPC_URL", "http://localhost:18082/json_rpc");
        env::set_var("MONERO_RPC_USER", "user");
        env::set_var("MONERO_RPC_PASS", "pass");
        env::set_var("MONITOR_START_HEIGHT", "42");
    }

    #[test]
    fn readiness_message_is_stable() {
        assert_eq!(
            workspace_ready_message(),
            "anon-ticket workspace scaffolding ready"
        );
    }

    #[test]
    fn config_loader_reads_env() {
        set_env();
        let config = BootstrapConfig::load_from_env().expect("config loads");
        assert_eq!(config.database_url(), "sqlite://test.db");
        assert_eq!(config.monitor_start_height(), 42);
    }

    #[test]
    fn pid_fingerprint_is_deterministic() {
        let left = derive_pid_fingerprint("abcd");
        let right = derive_pid_fingerprint("abcd");
        assert_eq!(left, right);
        assert_eq!(left.len(), 64);
    }
}
