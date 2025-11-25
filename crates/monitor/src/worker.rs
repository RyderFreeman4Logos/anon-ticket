// 引入标准库的时间处理功能，用于控制轮询间隔。
use std::time::Duration;

// 引入 metrics 库，用于通过 Prometheus 等工具监控服务运行状态。
// counter: 累加器，gauge: 仪表盘（可增可减），histogram: 直方图（分布统计）。
use metrics::{counter, gauge, histogram};
// 引入 `thiserror` 宏，用于方便地定义自定义错误枚举。
use thiserror::Error;
// 引入 tokio 的异步 sleep 函数，用于非阻塞等待。
use tokio::time::sleep;
// 引入 tracing 库的 `warn` 宏，用于日志记录。
use tracing::warn;

// 引入领域模型中的配置错误、遥测错误和存储层相关的接口与错误。
use anon_ticket_domain::{
    config::ConfigError,
    services::telemetry::TelemetryError,
    storage::{MonitorStateStore, PaymentStore, StorageError},
};

// 引入内部模块：
// `pipeline::process_entry`：处理单条转账的核心逻辑。
// `rpc::TransferSource`, `TransfersResponse`：数据源接口和响应结构。
use crate::{
    pipeline::process_entry,
    rpc::{TransferSource, TransfersResponse},
};

// 定义 `MonitorError` 枚举，它统一了 Monitor 服务中可能出现的所有错误类型。
// 使用 `#[from]` 宏可以自动实现 `From` trait，方便错误转换（例如 `?` 操作符）。
#[derive(Debug, Error)]
pub enum MonitorError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),     // 配置相关错误
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),   // 数据库/存储相关错误
    #[error("rpc error: {0}")]
    Rpc(String),                     // RPC 通信错误，这里直接包装字符串信息
    #[error("telemetry error: {0}")]
    Telemetry(#[from] TelemetryError), // 遥测/监控系统错误
}

// 定义主运行函数 `run_monitor`。
// 这是一个长时间运行的异步任务，负责循环轮询区块链数据。
// 泛型 S: 实现了 `TransferSource`，用于获取转账数据。
// 泛型 D: 实现了 `MonitorStateStore` 和 `PaymentStore`，用于保存状态和支付数据。
pub async fn run_monitor<S, D>(
    config: anon_ticket_domain::config::BootstrapConfig, // 启动配置
    storage: D,                                          // 存储层实例
    source: S,                                           // 数据源实例
) -> Result<(), MonitorError>
where
    S: TransferSource,
    D: MonitorStateStore + PaymentStore,
{
    // 初始化扫描高度。
    // 1. 尝试从存储中获取“上次处理的高度”(`last_processed_height`)。
    // 2. 如果存储中没有记录（例如首次运行），则使用配置中的 `monitor_start_height`。
    let mut height = storage
        .last_processed_height()
        .await?
        .unwrap_or(config.monitor_start_height());
    
    // 获取最小支付金额阈值配置。
    let min_payment_amount = config.monitor_min_payment_amount();

    // 进入主循环，持续监控。
    loop {
        // 调用 source 获取从 `height` 开始的转账记录。
        match source.fetch_transfers(height).await {
            Ok(transfers) => {
                // 如果获取成功，调用 `handle_batch` 处理这批数据。
                // 如果 `handle_batch` 失败，记录警告日志，但不要崩溃进程。
                // 这样在下一个循环周期可以重试。
                if let Err(err) = handle_batch(
                    &storage,
                    &source,
                    transfers,
                    &mut height, // 传入可变引用，以便内部更新高度
                    min_payment_amount,
                )
                .await
                {
                    warn!(?err, "batch processing failed, retrying in next cycle");
                }
            }
            Err(err) => {
                // 如果 RPC 调用本身失败（例如网络问题），记录指标和警告。
                counter!("monitor_rpc_calls_total", 1, "result" => "error");
                warn!(?err, "rpc fetch failed");
            }
        }
        // 休眠 5 秒，避免过于频繁地请求 RPC 接口。
        sleep(Duration::from_secs(5)).await;
    }
}

// 处理一批转账记录的辅助函数。
// 职责：处理每个条目，计算并更新下一个扫描高度。
async fn handle_batch<S, D>(
    storage: &D,
    source: &S,
    transfers: TransfersResponse,
    current_height: &mut u64, // 当前扫描高度的可变引用
    min_payment_amount: i64,
) -> Result<(), MonitorError>
where
    S: TransferSource,
    D: MonitorStateStore + PaymentStore,
{
    // 记录 RPC 调用成功的指标。
    counter!("monitor_rpc_calls_total", 1, "result" => "ok");
    // 记录这一批次包含的条目数量分布。
    histogram!("monitor_batch_entries", transfers.incoming.len() as f64);

    // 变量 `observed_height` 用于跟踪这批数据中观察到的最大区块高度。
    let mut observed_height: Option<u64> = None;

    // 遍历这批数据中的每一个条目。
    for entry in &transfers.incoming {
        // 如果条目包含高度信息（已确认交易），更新 `observed_height`。
        if let Some(h) = entry.height {
            let h = h as u64;
            // 取当前观察到的最大值。
            observed_height = Some(observed_height.map_or(h, |current| current.max(h)));
        }
        // 调用 `pipeline::process_entry` 处理单个条目（验证并存储）。
        process_entry(storage, entry, min_payment_amount).await?;
    }

    // 计算下一次扫描的高度。
    let mut next_height = *current_height;
    
    if let Some(max_height) = observed_height {
        // 策略 1: 如果在这批数据中观察到了新的区块高度，
        // 将下一次扫描高度设置为 `max_height + 1`，避免重复扫描。
        next_height = max_height + 1;
    } else if let Ok(chain_height) = source.wallet_height().await {
        // 策略 2: 如果这批数据为空（没有新交易），或者都在内存池中（无高度），
        // 我们尝试查询钱包当前的同步高度。
        // 取 `chain_height` 和当前 `next_height` 的较大值，推进进度。
        // 这确保即使没有交易，监控器也会跟随区块链高度向前移动。
        next_height = chain_height.max(next_height);
    }

    // 将更新后的高度持久化到数据库中，防止重启后回滚太远。
    storage.upsert_last_processed_height(next_height).await?;
    
    // 更新仪表盘指标，显示当前处理进度。
    gauge!("monitor_last_height", next_height as f64);
    
    // 更新内存中的 `current_height` 变量。
    *current_height = next_height;
    
    Ok(())
}

// 单元测试模块
#[cfg(test)]
mod tests {
    use super::*;
    use anon_ticket_domain::model::{ClaimOutcome, NewPayment, PaymentId, PaymentRecord};
    use anon_ticket_domain::storage::{PaymentStore, StorageResult};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // 定义 Mock 存储。
    #[derive(Clone)]
    struct MockStorage {
        // 控制模拟故障的开关。
        should_fail: Arc<AtomicBool>,
    }

    #[async_trait]
    impl MonitorStateStore for MockStorage {
        // 模拟读取最后处理高度
        async fn last_processed_height(&self) -> StorageResult<Option<u64>> {
            Ok(Some(100))
        }
        // 模拟更新最后处理高度
        async fn upsert_last_processed_height(&self, _height: u64) -> StorageResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl PaymentStore for MockStorage {
        // 模拟插入支付。如果 `should_fail` 为真，则返回错误。
        async fn insert_payment(&self, _payment: NewPayment) -> StorageResult<()> {
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(StorageError::Database("simulated failure".into()));
            }
            Ok(())
        }
        async fn claim_payment(&self, _pid: &PaymentId) -> StorageResult<Option<ClaimOutcome>> {
            Ok(None)
        }
        async fn find_payment(&self, _pid: &PaymentId) -> StorageResult<Option<PaymentRecord>> {
            Ok(None)
        }
    }

    // 定义 Mock 数据源。
    struct MockSource;

    #[async_trait]
    impl TransferSource for MockSource {
        async fn fetch_transfers(
            &self,
            _start_height: u64,
        ) -> Result<TransfersResponse, MonitorError> {
            Ok(TransfersResponse { incoming: vec![] })
        }
        async fn wallet_height(&self) -> Result<u64, MonitorError> {
            Ok(100)
        }
    }

    // 测试用例：验证 `handle_batch` 是否正确传播存储层的错误。
    #[tokio::test]
    async fn handle_batch_propagates_storage_error() {
        let should_fail = Arc::new(AtomicBool::new(true));
        let storage = MockStorage {
            should_fail: should_fail.clone(),
        };
        let source = MockSource;
        let mut height = 100;

        // 构造一个包含测试数据的响应。
        let transfers = TransfersResponse {
            incoming: vec![crate::rpc::TransferEntry {
                txid: "tx1".into(),
                payment_id: Some("1111111111111111".into()),
                amount: 100,
                height: Some(101),
                timestamp: 0,
            }],
        };

        // 场景 1: 存储层配置为失败。
        let result = handle_batch(&storage, &source, transfers.clone(), &mut height, 1).await;
        // 断言：结果应为 Error。
        assert!(result.is_err());

        // 场景 2: 存储层配置为成功。
        should_fail.store(false, Ordering::SeqCst);
        let result = handle_batch(&storage, &source, transfers, &mut height, 1).await;
        // 断言：结果应为 Ok。
        assert!(result.is_ok());
    }
}