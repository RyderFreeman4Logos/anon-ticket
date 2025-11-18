//! Domain-level building blocks shared across API and monitor crates.
//!
//! The current placeholder focuses on proving that the multi-crate workspace
//! is wired up correctly while already enforcing deterministic configuration
//! loading and hashing helpers shared between binaries.

mod cache;
mod telemetry;

use std::env;

use hex::encode as hex_encode;
use sha3::{Digest, Sha3_256};
use thiserror::Error;

pub use cache::*;
pub use telemetry::*;

/// API-specific configuration (HTTP bind + shared database) so the HTTP
/// surface does not depend on monitor-only environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiConfig {
    database_url: String,
    api_bind_address: String,
    api_unix_socket: Option<String>,
    internal_bind_address: Option<String>,
    internal_unix_socket: Option<String>,
}

impl ApiConfig {
    /// Loads only the environment variables required by the API binary.
    pub fn load_from_env() -> Result<Self, ConfigError> {
        hydrate_env_file()?;

        Ok(Self {
            database_url: get_required_var("DATABASE_URL")?,
            api_bind_address: get_required_var("API_BIND_ADDRESS")?,
            api_unix_socket: get_optional_var("API_UNIX_SOCKET"),
            internal_bind_address: get_optional_var("API_INTERNAL_BIND_ADDRESS"),
            internal_unix_socket: get_optional_var("API_INTERNAL_UNIX_SOCKET"),
        })
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn api_bind_address(&self) -> &str {
        &self.api_bind_address
    }

    pub fn api_unix_socket(&self) -> Option<&str> {
        self.api_unix_socket.as_deref()
    }

    pub fn internal_bind_address(&self) -> Option<&str> {
        self.internal_bind_address.as_deref()
    }

    pub fn internal_unix_socket(&self) -> Option<&str> {
        self.internal_unix_socket.as_deref()
    }

    pub fn has_internal_listener(&self) -> bool {
        self.internal_bind_address.is_some() || self.internal_unix_socket.is_some()
    }
}

/// Key configuration derived from `.env`/process variables so binaries can
/// share a deterministic environment contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapConfig {
    database_url: String,
    api_bind_address: String,
    monero_rpc_url: String,
    monitor_start_height: u64,
}

impl BootstrapConfig {
    /// Loads configuration by hydrating `.env` (if present) and reading the
    /// required process variables. Missing or malformed entries surface as
    /// `ConfigError` so binaries can respond gracefully.
    pub fn load_from_env() -> Result<Self, ConfigError> {
        hydrate_env_file()?;

        let database_url = get_required_var("DATABASE_URL")?;
        let api_bind_address = get_required_var("API_BIND_ADDRESS")?;
        let monero_rpc_url = get_required_var("MONERO_RPC_URL")?;
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

    pub fn monitor_start_height(&self) -> u64 {
        self.monitor_start_height
    }
}

fn get_required_var(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError::MissingVar { key })
}

fn get_optional_var(key: &'static str) -> Option<String> {
    env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(crate) fn hydrate_env_file() -> Result<(), ConfigError> {
    if env::var_os("ANON_TICKET_SKIP_DOTENV").is_some() {
        return Ok(());
    }
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(ConfigError::Dotenv { source: err }),
    }

    Ok(())
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

pub mod storage {
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use thiserror::Error;

    /// Common result alias for storage operations.
    pub type StorageResult<T> = Result<T, StorageError>;

    #[derive(Debug, Error, Clone, PartialEq, Eq)]
    pub enum StorageError {
        #[error("database error: {0}")]
        Database(String),
    }

    impl StorageError {
        pub fn from_source(err: impl std::fmt::Display) -> Self {
            Self::Database(err.to_string())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct PaymentId(String);

    impl PaymentId {
        pub fn new(value: impl Into<String>) -> Self {
            let mut owned = value.into();
            owned.make_ascii_lowercase();
            Self(owned)
        }

        pub fn parse(pid: &str) -> Result<Self, crate::PidFormatError> {
            crate::validate_pid(pid)?;
            Ok(Self::new(pid))
        }

        pub fn as_str(&self) -> &str {
            &self.0
        }

        pub fn into_inner(self) -> String {
            self.0
        }
    }

    impl From<&str> for PaymentId {
        fn from(value: &str) -> Self {
            Self::new(value.to_owned())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct ServiceToken(String);

    impl ServiceToken {
        pub fn new(value: impl Into<String>) -> Self {
            Self(value.into())
        }

        pub fn as_str(&self) -> &str {
            &self.0
        }

        pub fn into_inner(self) -> String {
            self.0
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PaymentStatus {
        Unclaimed,
        Claimed,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PaymentRecord {
        pub pid: PaymentId,
        pub txid: String,
        pub amount: i64,
        pub block_height: i64,
        pub status: PaymentStatus,
        pub created_at: DateTime<Utc>,
        pub claimed_at: Option<DateTime<Utc>>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NewPayment {
        pub pid: PaymentId,
        pub txid: String,
        pub amount: i64,
        pub block_height: i64,
        pub detected_at: DateTime<Utc>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ClaimOutcome {
        pub pid: PaymentId,
        pub txid: String,
        pub amount: i64,
        pub block_height: i64,
        pub claimed_at: DateTime<Utc>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NewServiceToken {
        pub token: ServiceToken,
        pub pid: PaymentId,
        pub amount: i64,
        pub issued_at: DateTime<Utc>,
        pub abuse_score: i16,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ServiceTokenRecord {
        pub token: ServiceToken,
        pub pid: PaymentId,
        pub amount: i64,
        pub issued_at: DateTime<Utc>,
        pub revoked_at: Option<DateTime<Utc>>,
        pub revoke_reason: Option<String>,
        pub abuse_score: i16,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RevokeTokenRequest {
        pub token: ServiceToken,
        pub reason: Option<String>,
        pub abuse_score: Option<i16>,
    }

    #[async_trait]
    pub trait PaymentStore: Send + Sync {
        async fn insert_payment(&self, payment: NewPayment) -> StorageResult<()>;
        async fn claim_payment(&self, pid: &PaymentId) -> StorageResult<Option<ClaimOutcome>>;
        async fn find_payment(&self, pid: &PaymentId) -> StorageResult<Option<PaymentRecord>>;
    }

    #[async_trait]
    pub trait TokenStore: Send + Sync {
        async fn insert_token(&self, token: NewServiceToken) -> StorageResult<ServiceTokenRecord>;
        async fn find_token(
            &self,
            token: &ServiceToken,
        ) -> StorageResult<Option<ServiceTokenRecord>>;
        async fn revoke_token(
            &self,
            request: RevokeTokenRequest,
        ) -> StorageResult<Option<ServiceTokenRecord>>;
    }

    #[async_trait]
    pub trait MonitorStateStore: Send + Sync {
        async fn last_processed_height(&self) -> StorageResult<Option<u64>>;
        async fn upsert_last_processed_height(&self, height: u64) -> StorageResult<()>;
    }
}

pub use storage::*;

/// Required length (in hex characters) for externally supplied payment IDs.
pub const PID_LENGTH: usize = 32;

/// Errors emitted when user-supplied payment IDs fail validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PidFormatError {
    #[error("payment id must be exactly {PID_LENGTH} hex characters")]
    WrongLength,
    #[error("payment id contains non-hex characters")]
    NonHex,
}

/// Validates that the supplied PID matches the 32 hex-character contract.
pub fn validate_pid(pid: &str) -> Result<(), PidFormatError> {
    if pid.len() != PID_LENGTH {
        return Err(PidFormatError::WrongLength);
    }

    if !pid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PidFormatError::NonHex);
    }

    Ok(())
}

/// Generates a deterministic SHA3-256 service token from the PID + TXID pair.
pub fn derive_service_token(pid: &PaymentId, txid: &str) -> ServiceToken {
    let mut hasher = Sha3_256::new();
    hasher.update(pid.as_str().as_bytes());
    hasher.update(txid.as_bytes());
    let digest = hasher.finalize();
    ServiceToken::new(hex_encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, sync::Mutex};

    static ENV_GUARD: Mutex<()> = Mutex::new(());

    fn set_env() {
        env::set_var("ANON_TICKET_SKIP_DOTENV", "1");
        env::set_var("DATABASE_URL", "sqlite://test.db");
        env::set_var("API_BIND_ADDRESS", "127.0.0.1:8080");
        env::remove_var("API_UNIX_SOCKET");
        env::remove_var("API_INTERNAL_BIND_ADDRESS");
        env::remove_var("API_INTERNAL_UNIX_SOCKET");
        env::set_var("MONERO_RPC_URL", "http://localhost:18082/json_rpc");
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
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        let config = BootstrapConfig::load_from_env().expect("config loads");
        assert_eq!(config.database_url(), "sqlite://test.db");
        assert_eq!(config.monitor_start_height(), 42);
    }

    #[test]
    fn api_config_only_requires_api_env() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        env::remove_var("MONERO_RPC_URL");
        env::remove_var("MONITOR_START_HEIGHT");
        env::set_var("DATABASE_URL", "sqlite://api-only.db");
        env::set_var("API_BIND_ADDRESS", "127.0.0.1:9999");

        let config = ApiConfig::load_from_env().expect("api config loads");
        assert_eq!(config.database_url(), "sqlite://api-only.db");
        assert_eq!(config.api_bind_address(), "127.0.0.1:9999");

        // Restore defaults for subsequent tests in this module.
        set_env();
    }

    #[test]
    fn api_config_supports_unix_and_internal_listeners() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        env::set_var("API_UNIX_SOCKET", "/tmp/api.sock");
        env::set_var("API_INTERNAL_BIND_ADDRESS", "127.0.0.1:9090");
        env::set_var("API_INTERNAL_UNIX_SOCKET", "/tmp/api-internal.sock");

        let config = ApiConfig::load_from_env().expect("config loads");
        assert_eq!(config.api_unix_socket(), Some("/tmp/api.sock"));
        assert_eq!(config.internal_bind_address(), Some("127.0.0.1:9090"));
        assert_eq!(
            config.internal_unix_socket(),
            Some("/tmp/api-internal.sock")
        );
        assert!(config.has_internal_listener());

        env::remove_var("API_UNIX_SOCKET");
        env::remove_var("API_INTERNAL_BIND_ADDRESS");
        env::remove_var("API_INTERNAL_UNIX_SOCKET");
        set_env();
    }

    #[test]
    fn pid_fingerprint_is_deterministic() {
        let left = derive_pid_fingerprint("abcd");
        let right = derive_pid_fingerprint("abcd");
        assert_eq!(left, right);
        assert_eq!(left.len(), 64);
    }

    #[test]
    fn pid_validation_rejects_invalid_inputs() {
        assert_eq!(validate_pid("deadbeef"), Err(PidFormatError::WrongLength));
        assert_eq!(
            validate_pid("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
            Err(PidFormatError::NonHex)
        );
        assert!(validate_pid("0123456789abcdef0123456789abcdef").is_ok());
    }

    #[test]
    fn payment_id_parse_checks_format() {
        assert!(PaymentId::parse("0123456789abcdef0123456789abcdef").is_ok());
        assert!(PaymentId::parse("not-valid").is_err());
    }

    #[test]
    fn payment_id_canonicalizes_case() {
        let pid = PaymentId::parse("ABCDEFABCDEFABCDEFABCDEFABCDEFAB").unwrap();
        assert_eq!(pid.as_str(), "abcdefabcdefabcdefabcdefabcdefab");

        let raw = PaymentId::new("FEDCBA9876543210FEDCBA9876543210");
        assert_eq!(raw.as_str(), "fedcba9876543210fedcba9876543210");
    }

    #[test]
    fn service_token_derivation_is_deterministic() {
        let pid = PaymentId::parse("0123456789abcdef0123456789abcdef").unwrap();
        let a = derive_service_token(&pid, "tx1");
        let b = derive_service_token(&pid, "tx1");
        assert_eq!(a.as_str(), b.as_str());
    }
}
