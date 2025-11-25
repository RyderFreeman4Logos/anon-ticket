//! Environment-driven configuration structures shared by all binaries.

use std::env;

use thiserror::Error;

/// API-specific configuration (HTTP bind + shared database) so the HTTP
/// surface does not depend on monitor-only environment variables.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiConfig {
    database_url: String,
    api_bind_address: String,
    api_unix_socket: Option<String>,
    internal_bind_address: Option<String>,
    internal_unix_socket: Option<String>,
    pid_cache_ttl_secs: Option<u64>,
    pid_cache_capacity: Option<u64>,
    pid_bloom_entries: Option<u64>,
    pid_bloom_fp_rate: Option<f64>,
}

impl ApiConfig {
    /// Loads only the environment variables required by the API binary.
    pub fn load_from_env() -> Result<Self, ConfigError> {
        let api_unix_socket = get_optional_var("API_UNIX_SOCKET");
        let internal_bind_address = get_optional_var("API_INTERNAL_BIND_ADDRESS");
        let internal_unix_socket = get_optional_var("API_INTERNAL_UNIX_SOCKET");

        if internal_bind_address.is_none() && internal_unix_socket.is_none() {
            return Err(ConfigError::MissingInternalListener);
        }

        Ok(Self {
            database_url: get_required_var("DATABASE_URL")?,
            api_bind_address: get_required_var("API_BIND_ADDRESS")?,
            api_unix_socket,
            internal_bind_address,
            internal_unix_socket,
            pid_cache_ttl_secs: get_optional_u64("API_PID_CACHE_TTL_SECS")?,
            pid_cache_capacity: get_optional_u64("API_PID_CACHE_CAPACITY")?,
            pid_bloom_entries: get_optional_u64("API_PID_BLOOM_ENTRIES")?,
            pid_bloom_fp_rate: get_optional_f64("API_PID_BLOOM_FP_RATE")?,
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

    pub fn pid_cache_ttl_secs(&self) -> Option<u64> {
        self.pid_cache_ttl_secs
    }

    pub fn pid_cache_capacity(&self) -> Option<u64> {
        self.pid_cache_capacity
    }

    pub fn pid_bloom_entries(&self) -> Option<u64> {
        self.pid_bloom_entries
    }

    pub fn pid_bloom_fp_rate(&self) -> Option<f64> {
        self.pid_bloom_fp_rate
    }
}

/// Key configuration derived from process variables so binaries can share a
/// deterministic environment contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapConfig {
    database_url: String,
    monero_rpc_url: String,
    monitor_start_height: u64,
    monitor_min_payment_amount: i64,
    monitor_poll_interval_secs: u64,
    monitor_min_confirmations: u64,
}

const DEFAULT_MIN_PAYMENT_AMOUNT: i64 = 1_000_000;
const DEFAULT_MONITOR_POLL_INTERVAL_SECS: u64 = 5;
const DEFAULT_MONITOR_MIN_CONFIRMATIONS: u64 = 10;

impl BootstrapConfig {
    /// Loads configuration by reading the required process variables. Missing
    /// or malformed entries surface as `ConfigError` so binaries can respond
    /// gracefully.
    pub fn load_from_env() -> Result<Self, ConfigError> {
        let database_url = get_required_var("DATABASE_URL")?;
        let monero_rpc_url = get_required_var("MONERO_RPC_URL")?;
        let monitor_start_height =
            get_required_var("MONITOR_START_HEIGHT")?
                .parse()
                .map_err(|source| ConfigError::InvalidNumber {
                    key: "MONITOR_START_HEIGHT",
                    source,
                })?;
        let monitor_min_payment_amount = get_optional_var("MONITOR_MIN_PAYMENT_AMOUNT")
            .map(|value| {
                value
                    .trim()
                    .parse()
                    .map_err(|source| ConfigError::InvalidNumber {
                        key: "MONITOR_MIN_PAYMENT_AMOUNT",
                        source,
                    })
            })
            .transpose()? // propagate parse errors
            .unwrap_or(DEFAULT_MIN_PAYMENT_AMOUNT);
        let monitor_poll_interval_secs = get_optional_var("MONITOR_POLL_INTERVAL_SECS")
            .map(|value| {
                value
                    .trim()
                    .parse()
                    .map_err(|source| ConfigError::InvalidNumber {
                        key: "MONITOR_POLL_INTERVAL_SECS",
                        source,
                    })
            })
            .transpose()? // propagate parse errors
            .unwrap_or(DEFAULT_MONITOR_POLL_INTERVAL_SECS);
        let monitor_min_confirmations = get_optional_var("MONITOR_MIN_CONFIRMATIONS")
            .map(|value| {
                value
                    .trim()
                    .parse()
                    .map_err(|source| ConfigError::InvalidNumber {
                        key: "MONITOR_MIN_CONFIRMATIONS",
                        source,
                    })
            })
            .transpose()? // propagate parse errors
            .unwrap_or(DEFAULT_MONITOR_MIN_CONFIRMATIONS);

        Ok(Self {
            database_url,
            monero_rpc_url,
            monitor_start_height,
            monitor_min_payment_amount,
            monitor_poll_interval_secs,
            monitor_min_confirmations,
        })
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn monero_rpc_url(&self) -> &str {
        &self.monero_rpc_url
    }

    pub fn monitor_start_height(&self) -> u64 {
        self.monitor_start_height
    }

    pub fn monitor_min_payment_amount(&self) -> i64 {
        self.monitor_min_payment_amount
    }

    pub fn monitor_poll_interval_secs(&self) -> u64 {
        self.monitor_poll_interval_secs
    }

    pub fn monitor_min_confirmations(&self) -> u64 {
        self.monitor_min_confirmations
    }
}

fn get_required_var(key: &'static str) -> Result<String, ConfigError> {
    match env::var(key) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(ConfigError::MissingVar { key })
            } else {
                Ok(trimmed.to_string())
            }
        }
        Err(_) => Err(ConfigError::MissingVar { key }),
    }
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

fn get_optional_u64(key: &'static str) -> Result<Option<u64>, ConfigError> {
    get_optional_var(key)
        .map(|value| {
            value
                .parse()
                .map_err(|source| ConfigError::InvalidNumber { key, source })
        })
        .transpose()
}

fn get_optional_f64(key: &'static str) -> Result<Option<f64>, ConfigError> {
    get_optional_var(key)
        .map(|value| {
            value
                .parse()
                .map_err(|source| ConfigError::InvalidFloat { key, source })
        })
        .transpose()
}

/// Errors emitted when environment parsing fails.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable `{key}`")]
    MissingVar { key: &'static str },
    #[error(
        "internal listener required: set API_INTERNAL_BIND_ADDRESS or API_INTERNAL_UNIX_SOCKET"
    )]
    MissingInternalListener,
    #[error("invalid integer in `{key}`: {source}")]
    InvalidNumber {
        key: &'static str,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("invalid float in `{key}`: {source}")]
    InvalidFloat {
        key: &'static str,
        #[source]
        source: std::num::ParseFloatError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_GUARD: Mutex<()> = Mutex::new(());

    fn set_env() {
        std::env::set_var("ANON_TICKET_SKIP_DOTENV", "1");
        std::env::set_var("DATABASE_URL", "sqlite://test.db");
        std::env::set_var("API_BIND_ADDRESS", "127.0.0.1:8080");
        std::env::remove_var("API_UNIX_SOCKET");
        std::env::set_var("API_INTERNAL_BIND_ADDRESS", "127.0.0.1:9090");
        std::env::remove_var("API_INTERNAL_UNIX_SOCKET");
        std::env::remove_var("API_PID_CACHE_TTL_SECS");
        std::env::remove_var("API_PID_CACHE_CAPACITY");
        std::env::remove_var("API_PID_BLOOM_ENTRIES");
        std::env::remove_var("API_PID_BLOOM_FP_RATE");
        std::env::set_var("MONERO_RPC_URL", "http://localhost:18082/json_rpc");
        std::env::set_var("MONITOR_START_HEIGHT", "42");
        std::env::remove_var("MONITOR_MIN_PAYMENT_AMOUNT");
        std::env::remove_var("MONITOR_POLL_INTERVAL_SECS");
        std::env::remove_var("MONITOR_MIN_CONFIRMATIONS");
    }

    #[test]
    fn api_config_only_requires_api_env() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::remove_var("MONERO_RPC_URL");
        std::env::remove_var("MONITOR_START_HEIGHT");
        std::env::set_var("DATABASE_URL", "sqlite://api-only.db");
        std::env::set_var("API_BIND_ADDRESS", "127.0.0.1:9999");
        std::env::set_var("API_INTERNAL_BIND_ADDRESS", "127.0.0.1:9998");

        let config = ApiConfig::load_from_env().expect("api config loads");
        assert_eq!(config.database_url(), "sqlite://api-only.db");
        assert_eq!(config.api_bind_address(), "127.0.0.1:9999");

        set_env();
    }

    #[test]
    fn api_config_supports_unix_and_internal_listeners() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("API_UNIX_SOCKET", "/tmp/api.sock");
        std::env::set_var("API_INTERNAL_BIND_ADDRESS", "127.0.0.1:9090");
        std::env::set_var("API_INTERNAL_UNIX_SOCKET", "/tmp/api-internal.sock");
        std::env::set_var("API_PID_CACHE_TTL_SECS", "120");
        std::env::set_var("API_PID_CACHE_CAPACITY", "200000");
        std::env::set_var("API_PID_BLOOM_ENTRIES", "500000");
        std::env::set_var("API_PID_BLOOM_FP_RATE", "0.01");

        let config = ApiConfig::load_from_env().expect("config loads");
        assert_eq!(config.api_unix_socket(), Some("/tmp/api.sock"));
        assert_eq!(config.internal_bind_address(), Some("127.0.0.1:9090"));
        assert_eq!(
            config.internal_unix_socket(),
            Some("/tmp/api-internal.sock")
        );
        assert!(config.has_internal_listener());
        assert_eq!(config.pid_cache_ttl_secs(), Some(120));
        assert_eq!(config.pid_cache_capacity(), Some(200_000));

        std::env::remove_var("API_UNIX_SOCKET");
        std::env::remove_var("API_INTERNAL_BIND_ADDRESS");
        std::env::remove_var("API_INTERNAL_UNIX_SOCKET");
        std::env::remove_var("API_PID_CACHE_TTL_SECS");
        std::env::remove_var("API_PID_CACHE_CAPACITY");
        std::env::remove_var("API_PID_BLOOM_ENTRIES");
        std::env::remove_var("API_PID_BLOOM_FP_RATE");
        set_env();
    }

    #[test]
    fn api_config_requires_internal_listener() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::remove_var("API_INTERNAL_BIND_ADDRESS");
        std::env::remove_var("API_INTERNAL_UNIX_SOCKET");

        let err = ApiConfig::load_from_env().unwrap_err();
        assert!(matches!(err, ConfigError::MissingInternalListener));

        set_env();
    }

    #[test]
    fn api_config_rejects_invalid_pid_cache_number() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("API_PID_CACHE_TTL_SECS", "abc");

        let err = ApiConfig::load_from_env().unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidNumber {
                key: "API_PID_CACHE_TTL_SECS",
                ..
            }
        ));

        set_env();
    }

    #[test]
    fn api_config_rejects_invalid_bloom_float() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("API_PID_BLOOM_FP_RATE", "not-a-float");

        let err = ApiConfig::load_from_env().unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidFloat {
                key: "API_PID_BLOOM_FP_RATE",
                ..
            }
        ));

        set_env();
    }

    #[test]
    fn required_env_vars_are_trimmed() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("DATABASE_URL", "  sqlite://trim.db  ");
        std::env::set_var("API_BIND_ADDRESS", " 127.0.0.1:8081 ");

        let config = ApiConfig::load_from_env().expect("config loads");
        assert_eq!(config.database_url(), "sqlite://trim.db");
        assert_eq!(config.api_bind_address(), "127.0.0.1:8081");

        set_env();
    }

    #[test]
    fn empty_required_env_var_is_treated_as_missing() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("DATABASE_URL", "   ");

        let err = ApiConfig::load_from_env().unwrap_err();
        assert!(matches!(
            err,
            ConfigError::MissingVar {
                key: "DATABASE_URL"
            }
        ));

        set_env();
    }

    #[test]
    fn config_loader_reads_env() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        let config = BootstrapConfig::load_from_env().expect("config loads");
        assert_eq!(config.database_url(), "sqlite://test.db");
        assert_eq!(config.monitor_start_height(), 42);
        assert_eq!(
            config.monitor_min_payment_amount(),
            DEFAULT_MIN_PAYMENT_AMOUNT
        );
        assert_eq!(
            config.monitor_poll_interval_secs(),
            DEFAULT_MONITOR_POLL_INTERVAL_SECS
        );
        assert_eq!(
            config.monitor_min_confirmations(),
            DEFAULT_MONITOR_MIN_CONFIRMATIONS
        );
    }

    #[test]
    fn monitor_min_payment_amount_overrides_default() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("MONITOR_MIN_PAYMENT_AMOUNT", " 2000000 ");

        let config = BootstrapConfig::load_from_env().expect("config loads");
        assert_eq!(config.monitor_min_payment_amount(), 2_000_000);

        set_env();
    }

    #[test]
    fn monitor_poll_interval_overrides_default() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("MONITOR_POLL_INTERVAL_SECS", " 10 ");

        let config = BootstrapConfig::load_from_env().expect("config loads");
        assert_eq!(config.monitor_poll_interval_secs(), 10);

        set_env();
    }

    #[test]
    fn monitor_min_confirmations_overrides_default() {
        let _guard = ENV_GUARD.lock().unwrap();
        set_env();
        std::env::set_var("MONITOR_MIN_CONFIRMATIONS", " 12 ");

        let config = BootstrapConfig::load_from_env().expect("config loads");
        assert_eq!(config.monitor_min_confirmations(), 12);

        set_env();
    }
}
