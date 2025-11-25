//! Monitor binary that tails monero-wallet-rpc for qualifying transfers.
//! 监控二进制程序：负责追踪 monero-wallet-rpc 以获取符合条件的转账。

// 声明模块结构。
// `pipeline`: 处理数据流逻辑。
// `rpc`: 处理与 Monero 钱包的 RPC 通信。
// `worker`: 包含主要的工作循环逻辑。
mod pipeline;
mod rpc;
mod worker;

use std::io;

// 引入配置管理、遥测服务和存储实现。
use anon_ticket_domain::config::BootstrapConfig;
use anon_ticket_domain::services::telemetry::{init_telemetry, TelemetryConfig};
use anon_ticket_storage::SeaOrmStorage;
use monero_rpc::RpcClientBuilder;

// 引入内部模块的类型。
use rpc::RpcTransferSource;
use worker::{run_monitor, MonitorError};

// `#[tokio::main]` 宏将 `main` 函数标记为 Tokio 运行时的入口点。
// 这允许我们在 `main` 函数中使用 `async/await` 语法。
#[tokio::main]
async fn main() -> io::Result<()> {
    // 调用 `bootstrap` 函数进行初始化和启动。
    // 如果失败，打印错误信息到标准错误输出，并返回 IO 错误。
    if let Err(err) = bootstrap().await {
        eprintln!("[monitor] bootstrap failed: {err}");
        return Err(io::Error::other(err.to_string()));
    }

    Ok(())
}

// 引导函数：负责系统的初始化、依赖注入和启动核心服务。
async fn bootstrap() -> Result<(), MonitorError> {
    // 1. 加载配置
    // 从环境变量或配置文件中加载 `BootstrapConfig`。
    let config = BootstrapConfig::load_from_env()?;

    // 2. 初始化遥测 (Telemetry)
    // 根据环境变量 "MONITOR" 前缀加载遥测配置，并初始化日志和监控系统。
    let telemetry_config = TelemetryConfig::from_env("MONITOR");
    init_telemetry(&telemetry_config)?;

    // 3. 连接数据库
    // 使用配置中的 URL 连接到 SeaOrm 兼容的数据库（如 Postgres）。
    // `SeaOrmStorage` 实现了 `MonitorStateStore` 和 `PaymentStore` trait。
    let storage = SeaOrmStorage::connect(config.database_url()).await?;

    // 4. 构建 RPC 客户端
    // 创建 `monero-rpc` 客户端，用于连接 Monero 钱包 RPC 服务。
    let rpc_client = RpcClientBuilder::new()
        .build(config.monero_rpc_url().to_string())
        .map_err(|err| MonitorError::Rpc(err.to_string()))?; // 错误转换
    
    // 获取钱包接口的句柄。
    let wallet = rpc_client.wallet();

    // 5. 创建数据源适配器
    // 将 `wallet` 客户端封装进 `RpcTransferSource`，使其符合 `TransferSource` trait。
    let source = RpcTransferSource::new(wallet);

    // 6. 启动监控循环
    // 将配置、存储和数据源注入 `run_monitor`，开始无限循环的任务。
    run_monitor(config, storage, source).await
}