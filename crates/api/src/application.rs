use std::{path::Path, sync::Arc, time::Duration};

#[cfg(unix)]
use std::fs;

use actix_web::{middleware::Logger, web, App, HttpServer};
use anon_ticket_domain::config::{ApiConfig, ConfigError};
use anon_ticket_domain::services::{
    cache::InMemoryPidCache,
    telemetry::{init_telemetry, TelemetryConfig, TelemetryError},
};
use anon_ticket_storage::SeaOrmStorage;
use thiserror::Error;

use crate::{
    handlers::{metrics_handler, redeem_handler, revoke_token_handler, token_status_handler},
    state::AppState,
};

const DEFAULT_PID_CACHE_NEGATIVE_GRACE_MS: u64 = 500;

pub async fn run() -> Result<(), BootstrapError> {
    let config = ApiConfig::load_from_env()?;
    let telemetry_config = TelemetryConfig::from_env("API");
    let telemetry = init_telemetry(&telemetry_config)?;
    let storage = SeaOrmStorage::connect(config.database_url()).await?;
    let cache_ttl = Duration::from_secs(
        config
            .pid_cache_ttl_secs()
            .unwrap_or_else(|| InMemoryPidCache::DEFAULT_TTL.as_secs()),
    );
    let cache_capacity = config
        .pid_cache_capacity()
        .unwrap_or(InMemoryPidCache::DEFAULT_CAPACITY);
    let negative_grace = Duration::from_millis(
        config
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
    let state = AppState::new(storage, cache, telemetry.clone(), negative_grace);

    let include_metrics_on_public = !config.has_internal_listener();
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
        if let Some(socket) = config.api_unix_socket() {
            cleanup_socket(socket)?;
            public_server = public_server.bind_uds(socket)?;
        } else {
            public_server = public_server.bind(config.api_bind_address())?;
        }
    }

    #[cfg(not(unix))]
    {
        if let Some(socket) = config.api_unix_socket() {
            return Err(BootstrapError::Io(std::io::Error::other(format!(
                "unix socket '{socket}' requested but this platform does not support it"
            ))));
        }
        public_server = public_server.bind(config.api_bind_address())?;
    }

    let public_server = public_server.run();

    let internal_server = if config.has_internal_listener() {
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
            if let Some(socket) = config.internal_unix_socket() {
                cleanup_socket(socket)?;
                internal_server = internal_server.bind_uds(socket)?;
            } else if let Some(addr) = config.internal_bind_address() {
                internal_server = internal_server.bind(addr)?;
            } else {
                return Err(BootstrapError::Io(std::io::Error::other(
                    "internal listener configured but no bind target provided",
                )));
            }
        }

        #[cfg(not(unix))]
        {
            if let Some(socket) = config.internal_unix_socket() {
                return Err(BootstrapError::Io(std::io::Error::other(format!(
                    "internal unix socket '{socket}' requested but this platform does not support it"
                ))));
            }
            if let Some(addr) = config.internal_bind_address() {
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
        tokio::try_join!(public_server, internal)?;
    } else {
        public_server.await?;
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("telemetry error: {0}")]
    Telemetry(#[from] TelemetryError),
    #[error("storage error: {0}")]
    Storage(#[from] anon_ticket_domain::storage::StorageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid cache configuration: {0}")]
    InvalidCacheConfig(String),
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
