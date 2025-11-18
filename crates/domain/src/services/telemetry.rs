use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use metrics::counter;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::OnceCell;
use thiserror::Error;
use tracing::warn;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::hydrate_env_file;

static SUBSCRIBER_INSTALLED: OnceCell<()> = OnceCell::new();
static METRICS_HANDLE: OnceCell<Arc<PrometheusHandle>> = OnceCell::new();

/// Shared observability options for binaries.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    log_filter: String,
    metrics_address: Option<String>,
    abuse_threshold: u16,
}

impl TelemetryConfig {
    /// Loads telemetry knobs from optional environment variables prefixed with
    /// `<PREFIX>_`, e.g. `API_LOG_FILTER`. Missing entries fall back to sane
    /// defaults so binaries do not require extra configuration to boot.
    pub fn from_env(prefix: &str) -> Self {
        let _ = hydrate_env_file();
        let upper = prefix.trim().to_ascii_uppercase();
        let log_key = format!("{}_LOG_FILTER", upper);
        let metrics_key = format!("{}_METRICS_ADDRESS", upper);
        let abuse_key = format!("{}_ABUSE_THRESHOLD", upper);

        let log_filter = env::var(log_key).unwrap_or_else(|_| "info".to_string());
        let metrics_address = env::var(metrics_key).ok().and_then(|value| {
            if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        });
        let abuse_threshold = env::var(abuse_key)
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(5);

        Self {
            log_filter,
            metrics_address,
            abuse_threshold,
        }
    }

    pub fn log_filter(&self) -> &str {
        &self.log_filter
    }

    pub fn metrics_address(&self) -> Option<&str> {
        self.metrics_address.as_deref()
    }

    pub fn abuse_threshold(&self) -> u16 {
        self.abuse_threshold
    }
}

/// Guard returned after telemetry initialization.
#[derive(Clone)]
pub struct TelemetryGuard {
    metrics: Arc<PrometheusHandle>,
    abuse_threshold: u16,
}

impl TelemetryGuard {
    pub fn render_metrics(&self) -> String {
        self.metrics.render()
    }

    pub fn abuse_threshold(&self) -> u16 {
        self.abuse_threshold
    }
}

/// Centralized helper to wire up tracing + metrics exporters once per process.
pub fn init_telemetry(config: &TelemetryConfig) -> Result<TelemetryGuard, TelemetryError> {
    install_tracing(config)?;
    let metrics = install_metrics(config)?;

    Ok(TelemetryGuard {
        metrics,
        abuse_threshold: config.abuse_threshold(),
    })
}

fn install_tracing(config: &TelemetryConfig) -> Result<(), TelemetryError> {
    if SUBSCRIBER_INSTALLED.get().is_some() {
        return Ok(());
    }

    let env_filter = EnvFilter::try_new(config.log_filter())
        .map_err(|err| TelemetryError::InvalidLogFilter(err.to_string()))?;

    if SUBSCRIBER_INSTALLED.set(()).is_ok() {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .try_init()
            .map_err(|err| TelemetryError::Tracing(err.to_string()))?;
    }

    Ok(())
}

fn install_metrics(config: &TelemetryConfig) -> Result<Arc<PrometheusHandle>, TelemetryError> {
    METRICS_HANDLE
        .get_or_try_init(|| {
            let mut builder = PrometheusBuilder::new();
            if let Some(addr) = config.metrics_address() {
                let socket: SocketAddr =
                    addr.parse().map_err(|err: std::net::AddrParseError| {
                        TelemetryError::InvalidMetricsAddress(addr.to_string(), err.to_string())
                    })?;
                builder = builder.with_http_listener(socket);
            }

            builder
                .install_recorder()
                .map(Arc::new)
                .map_err(|err| TelemetryError::Metrics(err.to_string()))
        })
        .cloned()
}

/// Tracks repeated invalid redemption attempts so callers exceeding the
/// threshold can be escalated or blocked.
#[derive(Clone, Default)]
pub struct AbuseTracker {
    threshold: u16,
    inner: Arc<Mutex<HashMap<String, AbuseRecord>>>,
}

impl AbuseTracker {
    pub fn new(threshold: u16) -> Self {
        Self {
            threshold: threshold.max(1),
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn record(&self, key: impl AsRef<str>) -> AbuseSignal {
        let key = key.as_ref().to_owned();
        let mut guard = self.inner.lock().expect("mutex poisoned");
        let entry = guard.entry(key.clone()).or_insert_with(|| AbuseRecord {
            attempts: 0,
            last_seen: Utc::now(),
        });
        entry.attempts = entry.attempts.saturating_add(1);
        entry.last_seen = Utc::now();
        counter!("abuse_events_total", 1, "state" => "probe");

        if entry.attempts >= self.threshold {
            counter!("abuse_events_total", 1, "state" => "escalated");
            warn!(
                identifier = key,
                attempts = entry.attempts,
                "abuse threshold exceeded"
            );
            AbuseSignal::Escalated {
                attempts: entry.attempts,
            }
        } else {
            AbuseSignal::None
        }
    }

    pub fn reset(&self, key: impl AsRef<str>) {
        let mut guard = self.inner.lock().expect("mutex poisoned");
        guard.remove(key.as_ref());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbuseSignal {
    None,
    Escalated { attempts: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbuseRecord {
    pub attempts: u16,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("invalid log filter: {0}")]
    InvalidLogFilter(String),
    #[error("failed to install tracing subscriber: {0}")]
    Tracing(String),
    #[error("invalid metrics address `{0}`: {1}")]
    InvalidMetricsAddress(String, String),
    #[error("failed to install metrics recorder: {0}")]
    Metrics(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_GUARD: Mutex<()> = Mutex::new(());

    #[test]
    fn telemetry_config_uses_defaults() {
        let _guard = ENV_GUARD.lock().unwrap();
        env::remove_var("API_LOG_FILTER");
        env::remove_var("API_METRICS_ADDRESS");
        env::remove_var("API_ABUSE_THRESHOLD");

        let cfg = TelemetryConfig::from_env("api");
        assert_eq!(cfg.log_filter(), "info");
        assert_eq!(cfg.metrics_address(), None);
        assert_eq!(cfg.abuse_threshold(), 5);
    }

    #[test]
    fn telemetry_config_reads_env() {
        let _guard = ENV_GUARD.lock().unwrap();
        env::set_var("API_LOG_FILTER", "debug");
        env::set_var("API_METRICS_ADDRESS", "127.0.0.1:9898");
        env::set_var("API_ABUSE_THRESHOLD", "9");
        let cfg = TelemetryConfig::from_env("API");
        assert_eq!(cfg.log_filter(), "debug");
        assert_eq!(cfg.metrics_address(), Some("127.0.0.1:9898"));
        assert_eq!(cfg.abuse_threshold(), 9);
        env::remove_var("API_LOG_FILTER");
        env::remove_var("API_METRICS_ADDRESS");
        env::remove_var("API_ABUSE_THRESHOLD");
    }

    #[test]
    fn empty_metrics_address_is_treated_as_none() {
        let _guard = ENV_GUARD.lock().unwrap();
        env::remove_var("API_METRICS_ADDRESS");
        env::set_var("API_METRICS_ADDRESS", "  ");
        let cfg = TelemetryConfig::from_env("API");
        assert_eq!(cfg.metrics_address(), None);
        env::remove_var("API_METRICS_ADDRESS");
    }

    #[test]
    fn abuse_tracker_escalates() {
        let tracker = AbuseTracker::new(2);
        assert_eq!(tracker.record("pid"), AbuseSignal::None);
        assert_eq!(
            tracker.record("pid"),
            AbuseSignal::Escalated { attempts: 2 }
        );
        tracker.reset("pid");
        assert_eq!(tracker.record("pid"), AbuseSignal::None);
    }
}
