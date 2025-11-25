// 引入标准库：
// `Path`: 文件路径处理。
// `Arc`: 原子引用计数，用于共享状态。
use std::{path::Path, sync::Arc};

// 仅在 Unix 系统下引入文件系统模块，用于处理 Unix Domain Socket 文件。
#[cfg(unix)]
use std::fs;

// 引入 actix-web 框架组件：
// `middleware::Logger`: HTTP 请求日志中间件。
// `web`: 路由配置和数据提取。
// `App`: 应用程序构造器。
// `HttpServer`: HTTP 服务器。
use actix_web::{middleware::Logger, web, App, HttpServer};

// 引入领域层配置和服务：
// `ApiConfig`: API 服务配置。
// `InMemoryPidCache`: 内存缓存。
// `init_telemetry`: 初始化遥测。
use anon_ticket_domain::config::{ApiConfig, ConfigError};
use anon_ticket_domain::services::{
    cache::InMemoryPidCache,
    telemetry::{init_telemetry, TelemetryConfig, TelemetryError},
};
// 引入存储层实现。
use anon_ticket_storage::SeaOrmStorage;
// 引入错误宏。
use thiserror::Error;

// 引入内部模块：
// 处理函数 handlers。
// 应用状态 AppState。
use crate::{
    handlers::{metrics_handler, redeem_handler, revoke_token_handler, token_status_handler},
    state::AppState,
};

// 应用程序启动入口函数。
// 返回 `Result<(), BootstrapError>`。
pub async fn run() -> Result<(), BootstrapError> {
    // 1. 加载配置
    let config = ApiConfig::load_from_env()?;
    
    // 2. 初始化遥测 (Telemetry)
    // 根据 "API" 前缀的环境变量配置遥测。
    let telemetry_config = TelemetryConfig::from_env("API");
    let telemetry = init_telemetry(&telemetry_config)?;

    // 3. 连接数据库
    let storage = SeaOrmStorage::connect(config.database_url()).await?;

    // 4. 初始化缓存
    // 创建一个默认的内存 PID 缓存，并用 Arc 包装。
    let cache = Arc::new(InMemoryPidCache::default());

    // 5. 构建应用状态
    // 将存储、缓存和遥测守卫组合成 AppState。
    let state = AppState::new(storage, cache, telemetry.clone());

    // 判断是否在公共接口上暴露指标端点。
    // 如果配置了内部监听器（Internal Listener），则通常只在内部接口暴露指标，公共接口不暴露。
    let include_metrics_on_public = !config.has_internal_listener();

    // 克隆 state 用于公共服务器闭包。
    let public_state = state.clone();

    // 6. 配置并创建公共 HTTP 服务器 (Public Server)
    // `move ||` 闭包会在每个 worker 线程中执行，构建 App 实例。
    let mut public_server = HttpServer::new(move || {
        let mut app = App::new()
            // 注入共享状态数据
            .app_data(web::Data::new(public_state.clone()))
            // 添加日志中间件
            .wrap(Logger::default())
            // 注册路由：
            // POST /api/v1/redeem -> 兑换处理
            .route("/api/v1/redeem", web::post().to(redeem_handler))
            // GET /api/v1/token/{token} -> 状态查询
            .route("/api/v1/token/{token}", web::get().to(token_status_handler));

        // 如果需要在公共接口暴露指标，注册 /metrics 路由。
        if include_metrics_on_public {
            app = app.route("/metrics", web::get().to(metrics_handler));
        }

        app
    });

    // 绑定公共服务器地址。
    // Unix 系统下支持 Unix Domain Socket (UDS)。
    #[cfg(unix)]
    {
        if let Some(socket) = config.api_unix_socket() {
            // 如果配置了 UDS，先清理可能存在的旧 socket 文件。
            cleanup_socket(socket)?;
            public_server = public_server.bind_uds(socket)?;
        } else {
            // 否则绑定 TCP 地址。
            public_server = public_server.bind(config.api_bind_address())?;
        }
    }

    // 非 Unix 系统（如 Windows）不支持 UDS。
    #[cfg(not(unix))]
    {
        if let Some(socket) = config.api_unix_socket() {
            return Err(BootstrapError::Io(std::io::Error::other(format!(
                "unix socket '{socket}' requested but this platform does not support it"
            ))));
        }
        public_server = public_server.bind(config.api_bind_address())?;
    }

    // 运行公共服务器（非阻塞，返回 Server 句柄）。
    let public_server = public_server.run();

    // 7. 配置并创建内部 HTTP 服务器 (Internal Server) - 可选
    // 内部服务器通常用于管理任务（如撤销令牌）和监控指标，不向公网暴露。
    let internal_server = if config.has_internal_listener() {
        let internal_state = state.clone();
        let mut internal_server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(internal_state.clone()))
                .wrap(Logger::default())
                // 内部接口始终暴露指标
                .route("/metrics", web::get().to(metrics_handler))
                // 内部管理接口：撤销令牌
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

    // 8. 并发运行服务器
    if let Some(internal) = internal_server {
        // 如果开启了内部服务器，使用 `try_join!` 同时等待两个服务器运行。
        // 任何一个出错都会导致整体退出。
        tokio::try_join!(public_server, internal)?;
    } else {
        // 否则只等待公共服务器。
        public_server.await?;
    }

    Ok(())
}

// 定义启动过程中的错误枚举。
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
}

// 辅助函数：清理 Unix Socket 文件。
// 如果 socket 文件已存在（例如上次非正常退出遗留），bind 会失败，所以需要先删除。
#[cfg(unix)]
fn cleanup_socket(path: &str) -> std::io::Result<()> {
    let socket_path = Path::new(path);
    if socket_path.exists() {
        fs::remove_file(socket_path)?;
    }
    Ok(())
}

// 非 Unix 系统的空实现。
#[cfg(not(unix))]
fn cleanup_socket(_path: &str) -> std::io::Result<()> {
    Ok(())
}

// 单元测试模块。
#[cfg(test)]
mod tests {
    // 测试 cleanup_socket 功能。
    #[cfg(unix)]
    #[actix_web::test]
    async fn cleanup_socket_removes_stale_file() {
        use super::cleanup_socket;

        // 创建一个唯一的临时文件路径。
        let path = std::env::temp_dir().join(format!(
            "anon-ticket-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // 创建一个伪造的 socket 文件。
        std::fs::write(&path, b"stub").expect("write socket file");
        // 调用清理函数。
        cleanup_socket(path.to_str().unwrap()).expect("cleanup succeeds");
        // 断言文件已被删除。
        assert!(!path.exists());
    }
}