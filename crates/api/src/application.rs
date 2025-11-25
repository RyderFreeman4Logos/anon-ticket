use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::fs;

use actix_web::{middleware::Logger, web, App, HttpServer};
use anon_ticket_domain::config::{ApiConfig, BootstrapConfig, ConfigError};
use anon_ticket_domain::services::{
    cache::{BloomConfigError, InMemoryPidCache, PidBloom},
    telemetry::{init_telemetry, TelemetryConfig, TelemetryError},
};
use anon_ticket_domain::PidCache;
use anon_ticket_monitor::{build_rpc_source, run_monitor, worker::MonitorHooks};
use anon_ticket_storage::SeaOrmStorage;
use thiserror::Error;
use tracing::{info, warn};

use crate::{
    handlers::{metrics_handler, redeem_handler, revoke_token_handler, token_status_handler},
    state::AppState,
};

const DEFAULT_PID_CACHE_NEGATIVE_GRACE_MS: u64 = 500;
const DEFAULT_PID_BLOOM_ENTRIES: u64 = 100_000;
const DEFAULT_PID_BLOOM_FP_RATE: f64 = 0.01;

pub async fn run() -> Result<(), BootstrapError> {
    let api_config = ApiConfig::load_from_env()?;
    let monitor_config = maybe_load_monitor_config()?;
    let telemetry_config = TelemetryConfig::from_env("API");
    let telemetry = init_telemetry(&telemetry_config)?;
    let storage = SeaOrmStorage::connect(api_config.database_url()).await?;
    let cache_ttl = Duration::from_secs(
        api_config
            .pid_cache_ttl_secs()
            .unwrap_or_else(|| InMemoryPidCache::DEFAULT_TTL.as_secs()),
    );
    let cache_capacity = api_config
        .pid_cache_capacity()
        .unwrap_or(InMemoryPidCache::DEFAULT_CAPACITY);
    let negative_grace = Duration::from_millis(
        api_config
            .pid_cache_negative_grace_ms()
            .unwrap_or(DEFAULT_PID_CACHE_NEGATIVE_GRACE_MS),
    );
    if cache_ttl < negative_grace {
        return Err(BootstrapError::InvalidCacheConfig(
            "API_PID_CACHE_TTL_SECS must be >= API_PID_CACHE_NEGATIVE_GRACE_MS (converted to seconds)"
                .to_string(),
        ));
    }
    let cache = Arc::new(InMemoryPidCache::with_capacity(cache_ttl, cache_capacity));
    let bloom_entries = api_config
        .pid_bloom_entries()
        .unwrap_or(DEFAULT_PID_BLOOM_ENTRIES);
    let bloom_fp = api_config
        .pid_bloom_fp_rate()
        .unwrap_or(DEFAULT_PID_BLOOM_FP_RATE);
    if bloom_entries == 0 && !allow_missing_bloom() {
        return Err(BootstrapError::InvalidBloomConfig(
            "Bloom filter is disabled (API_PID_BLOOM_ENTRIES=0) but API_ALLOW_NO_BLOOM is not set"
                .to_string(),
        ));
    }
    let bloom = build_bloom_filter(Some(bloom_entries), Some(bloom_fp))?.map(Arc::new);
    info!(bloom_entries, bloom_fp, "configured pid bloom filter");

    prewarm_hints(&storage, &cache, bloom.as_deref()).await?;

    let monitor_hooks = MonitorHooks::new(
        Some(cache.clone() as Arc<dyn anon_ticket_domain::PidCache>),
        bloom.clone(),
    );

    let monitor_task = if let Some(cfg) = monitor_config {
        let storage_clone = storage.clone();
        let hooks = monitor_hooks.clone();
        let source = build_rpc_source(cfg.monero_rpc_url())?;
        Some(tokio::spawn(async move {
            run_monitor(cfg, storage_clone, source, Some(hooks)).await
        }))
    } else {
        None
    };

    let state = AppState::new(storage, cache, telemetry.clone(), negative_grace, bloom);

    let include_metrics_on_public = !api_config.has_internal_listener();
    let public_state = state.clone();
    let mut public_server = HttpServer::new(move || {
        let mut app = App::new()
            .app_data(web::Data::new(public_state.clone()))
            .wrap(Logger::default())
            .route("/api/v1/redeem", web::post().to(redeem_handler))
            .route("/api/v1/token/{token}", web::get().to(token_status_handler));

        if include_metrics_on_public {
            app = app.route("/metrics", web::get().to(metrics_handler));
        }

        app
    });

    #[cfg(unix)]
    {
        if let Some(socket) = api_config.api_unix_socket() {
            cleanup_socket(socket)?;
            public_server = public_server.bind_uds(socket)?;
        } else {
            public_server = public_server.bind(api_config.api_bind_address())?;
        }
    }

    #[cfg(not(unix))]
    {
        if let Some(socket) = api_config.api_unix_socket() {
            return Err(BootstrapError::Io(std::io::Error::other(format!(
                "unix socket '{socket}' requested but this platform does not support it"
            ))));
        }
        public_server = public_server.bind(api_config.api_bind_address())?;
    }

    let public_server = public_server.run();

    let internal_server = if api_config.has_internal_listener() {
        let internal_state = state.clone();
        let mut internal_server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(internal_state.clone()))
                .wrap(Logger::default())
                .route("/metrics", web::get().to(metrics_handler))
                .route(
                    "/api/v1/token/{token}/revoke",
                    web::post().to(revoke_token_handler),
                )
        });

        #[cfg(unix)]
        {
            if let Some(socket) = api_config.internal_unix_socket() {
                cleanup_socket(socket)?;
                internal_server = internal_server.bind_uds(socket)?;
            } else if let Some(addr) = api_config.internal_bind_address() {
                internal_server = internal_server.bind(addr)?;
            } else {
                return Err(BootstrapError::Io(std::io::Error::other(
                    "internal listener configured but no bind target provided",
                )));
            }
        }

        #[cfg(not(unix))]
        {
            if let Some(socket) = api_config.internal_unix_socket() {
                return Err(BootstrapError::Io(std::io::Error::other(format!(
                    "internal unix socket '{socket}' requested but this platform does not support it"
                ))));
            }
            if let Some(addr) = api_config.internal_bind_address() {
                internal_server = internal_server.bind(addr)?;
            } else {
                return Err(BootstrapError::Io(std::io::Error::other(
                    "internal listener configured but no bind target provided",
                )));
            }
        }

        Some(internal_server.run())
    } else {
        None
    };

    if let Some(internal) = internal_server {
        if let Some(monitor_handle) = monitor_task {
            tokio::try_join!(
                async { public_server.await.map_err(BootstrapError::Io) },
                async { internal.await.map_err(BootstrapError::Io) },
                monitor_join(monitor_handle),
            )?;
        } else {
            tokio::try_join!(
                async { public_server.await.map_err(BootstrapError::Io) },
                async { internal.await.map_err(BootstrapError::Io) },
            )?;
        }
    } else if let Some(monitor_handle) = monitor_task {
        tokio::try_join!(
            async { public_server.await.map_err(BootstrapError::Io) },
            monitor_join(monitor_handle),
        )?;
    } else {
        public_server.await?;
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("monitor config error: {0}")]
    MonitorConfig(ConfigError),
    #[error("telemetry error: {0}")]
    Telemetry(#[from] TelemetryError),
    #[error("storage error: {0}")]
    Storage(#[from] anon_ticket_domain::storage::StorageError),
    #[error("monitor error: {0}")]
    Monitor(#[from] anon_ticket_monitor::worker::MonitorError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid cache configuration: {0}")]
    InvalidCacheConfig(String),
    #[error("invalid bloom filter configuration: {0}")]
    InvalidBloomConfig(String),
    #[error("task join error: {0}")]
    Join(String),
}

#[cfg(unix)]
fn cleanup_socket(path: &str) -> std::io::Result<()> {
    let socket_path = Path::new(path);
    if socket_path.exists() {
        fs::remove_file(socket_path)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn cleanup_socket(_path: &str) -> std::io::Result<()> {
    Ok(())
}

fn build_bloom_filter(
    entries: Option<u64>,
    fp_rate: Option<f64>,
) -> Result<Option<PidBloom>, BootstrapError> {
    let entries = entries.unwrap_or(DEFAULT_PID_BLOOM_ENTRIES);
    if entries == 0 {
        return Ok(None);
    }
    let fp = fp_rate.unwrap_or(DEFAULT_PID_BLOOM_FP_RATE);
    if !(0.0..1.0).contains(&fp) {
        return Err(BootstrapError::InvalidBloomConfig(
            "API_PID_BLOOM_FP_RATE must be between 0 and 1".to_string(),
        ));
    }
    PidBloom::new(entries, fp)
        .map(Some)
        .map_err(|err| match err {
            BloomConfigError::InvalidEntries => {
                BootstrapError::InvalidBloomConfig("API_PID_BLOOM_ENTRIES must be > 0".into())
            }
            BloomConfigError::InvalidFalsePositiveRate(rate) => BootstrapError::InvalidBloomConfig(
                format!("API_PID_BLOOM_FP_RATE must be in (0,1): {rate}"),
            ),
        })
}

async fn prewarm_hints(
    storage: &SeaOrmStorage,
    cache: &InMemoryPidCache,
    bloom: Option<&PidBloom>,
) -> Result<(), BootstrapError> {
    let start = Instant::now();
    let pids = storage.all_payment_ids().await?;
    for pid in &pids {
        cache.mark_present(pid);
        if let Some(b) = bloom {
            b.insert(pid);
        }
    }
    info!(
        count = pids.len(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "prefilled cache/bloom with existing payments",
    );
    Ok(())
}

fn maybe_load_monitor_config() -> Result<Option<BootstrapConfig>, BootstrapError> {
    match BootstrapConfig::load_from_env() {
        Ok(cfg) => Ok(Some(cfg)),
        Err(err) if allow_missing_monitor() => {
            warn!(
                ?err,
                "monitor config missing; embedded monitor disabled (API_ALLOW_NO_MONITOR=1)"
            );
            Ok(None)
        }
        Err(err) => Err(BootstrapError::MonitorConfig(err)),
    }
}

fn allow_missing_monitor() -> bool {
    env_truthy("API_ALLOW_NO_MONITOR")
}

fn allow_missing_bloom() -> bool {
    env_truthy("API_ALLOW_NO_BLOOM")
}

async fn monitor_join(
    handle: tokio::task::JoinHandle<Result<(), anon_ticket_monitor::worker::MonitorError>>,
) -> Result<(), BootstrapError> {
    handle
        .await
        .map_err(|err| BootstrapError::Join(err.to_string()))??;
    Ok(())
}

fn env_truthy(key: &str) -> bool {
    matches!(std::env::var(key), Ok(val) if val == "1" || val.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[actix_web::test]
    async fn cleanup_socket_removes_stale_file() {
        use super::cleanup_socket;

        let path = std::env::temp_dir().join(format!(
            "anon-ticket-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"stub").expect("write socket file");
        cleanup_socket(path.to_str().unwrap()).expect("cleanup succeeds");
        assert!(!path.exists());
    }
}
