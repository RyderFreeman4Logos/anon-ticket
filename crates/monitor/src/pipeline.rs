// 引入领域模型中的 `NewPayment`（用于创建新支付记录）和 `PaymentId`（支付标识符）。
use anon_ticket_domain::model::{NewPayment, PaymentId};
// 引入 `PaymentStore` trait，它是持久化层（数据库）的接口，定义了如何存储支付信息。
use anon_ticket_domain::storage::PaymentStore;
// 引入时间处理库 chrono，用于处理日期和时间。
use chrono::{DateTime, Utc};
// 引入 metrics 库的 `counter` 宏，用于记录系统指标（如计数器），方便监控。
use metrics::counter;
// 引入 tracing 库的 `warn` 宏，用于记录警告级别的日志。
use tracing::warn;

// 引入内部定义的 `TransferEntry` 结构体，代表从 RPC 获取的单条转账记录。
use crate::rpc::TransferEntry;
// 引入内部定义的错误类型 `MonitorError`。
use crate::worker::MonitorError;

// 定义核心处理函数 `process_entry`。
// 这是一个异步泛型函数，用于处理单条转账记录。
// 泛型参数 `S` 必须实现 `PaymentStore` trait，这允许我们在测试时传入 Mock 对象，在生产时传入真实数据库。
pub async fn process_entry<S>(
    storage: &S,             // 存储层的引用
    entry: &TransferEntry,   // 要处理的转账记录
    min_payment_amount: i64, // 系统配置的最小支付金额阈值
) -> Result<bool, MonitorError>
where
    S: PaymentStore,
{
    // 步骤 1：初步验证
    // 使用模式匹配解构 `entry.payment_id` 和 `entry.height`。
    // 如果任何一个为 `None`（即没有 Payment ID 或不在区块中/未确认），则 `else` 分支会被执行。
    // 这里的逻辑意味着我们只处理包含 Payment ID 且已被打包进区块的交易。
    let (Some(pid), Some(height)) = (&entry.payment_id, entry.height) else {
        // 如果条件不满足，直接返回 `Ok(false)`，表示该条目被忽略，未被处理/存储。
        return Ok(false);
    };

    // 步骤 2：金额检查
    // 检查交易金额是否小于配置的最小阈值。
    if entry.amount < min_payment_amount {
        // 如果是“粉尘攻击”或金额不足的支付，记录一条警告日志。
        // 日志中包含了具体的金额、阈值和交易 ID，方便排查。
        warn!(
            amount = entry.amount,
            min_payment_amount,
            txid = entry.txid,
            "skipping dust payment below minimum amount" // "跳过低于最小金额的粉尘支付"
        );
        // 更新监控指标 `monitor_payments_ingested_total`，标签 result 为 "dust"。
        counter!(
            "monitor_payments_ingested_total",
            1,
            "result" => "dust"
        );
        // 返回 `Ok(false)` 表示忽略该支付。
        return Ok(false);
    }

    // 步骤 3：准备数据
    // 将 Unix 时间戳转换为 `DateTime<Utc>` 对象。
    // 如果转换失败（例如时间戳非法），则默认使用当前时间 `Utc::now`。
    let detected_at = DateTime::from_timestamp(entry.timestamp as i64, 0).unwrap_or_else(Utc::now);
    
    // 解析 Payment ID 字符串为强类型的 `PaymentId` 对象。
    let pid = match PaymentId::parse(pid) {
        Ok(pid) => pid,
        Err(_) => {
            // 如果 Payment ID 格式非法，记录日志并更新指标。
            warn!(pid, "skipping invalid pid");
            counter!("monitor_payments_ingested_total", 1, "result" => "invalid_pid");
            return Ok(false);
        }
    };

    // 步骤 4：持久化存储
    // 调用存储层的 `insert_payment` 方法将合法的支付记录写入数据库。
    // 构建 `NewPayment` 结构体作为参数。
    storage
        .insert_payment(NewPayment {
            pid,
            txid: entry.txid.clone(), // 复制交易 ID
            amount: entry.amount,
            block_height: height,
            detected_at,
        })
        .await?; // 使用 `?` 操作符，如果数据库操作失败，错误会自动向上传播。

    // 如果成功写入，更新监控指标，result 标记为 "persisted"（已持久化）。
    counter!("monitor_payments_ingested_total", 1, "result" => "persisted");

    // 返回 `Ok(true)` 表示该条目已被成功处理并存储。
    Ok(true)
}

// 单元测试模块
#[cfg(test)]
mod tests {
    use super::*; // 引入上层模块内容
    // 引入测试所需的额外类型
    use anon_ticket_domain::model::{ClaimOutcome, PaymentRecord};
    use anon_ticket_domain::storage::{PaymentStore, StorageResult};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // 定义一个 Mock 存储结构体，用于测试环境。
    // 它使用原子计数器 `AtomicUsize` 来记录插入操作调用的次数。
    #[derive(Clone, Default)]
    struct MockStorage {
        inserted: Arc<AtomicUsize>,
    }

    // 为 MockStorage 实现 `PaymentStore` trait。
    // 这样它就可以作为参数传递给 `process_entry` 函数。
    #[async_trait]
    impl PaymentStore for MockStorage {
        // 模拟插入支付记录
        async fn insert_payment(&self, _payment: NewPayment) -> StorageResult<()> {
            // 每次调用增加计数器
            self.inserted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        // 其他不需要的方法返回空实现或默认值
        async fn claim_payment(&self, _pid: &PaymentId) -> StorageResult<Option<ClaimOutcome>> {
            Ok(None)
        }

        async fn find_payment(&self, _pid: &PaymentId) -> StorageResult<Option<PaymentRecord>> {
            Ok(None)
        }
    }

    // 辅助函数：生成测试用的 `TransferEntry` 样本。
    fn sample_entry(amount: i64) -> TransferEntry {
        TransferEntry {
            txid: "tx1".to_string(),
            amount,
            height: Some(10), // 假设已确认
            timestamp: 0,
            payment_id: Some("1111111111111111".to_string()), // 有效的 Payment ID
        }
    }

    // 测试用例：验证是否会跳过低于阈值的“粉尘”支付。
    #[tokio::test]
    async fn skips_dust_below_threshold() {
        let storage = MockStorage::default();
        let min_payment_amount = 10;

        // 调用被测函数，传入金额为 5 的样本（低于阈值 10）。
        let result = process_entry(&storage, &sample_entry(5), min_payment_amount)
            .await
            .expect("processing succeeds");

        // 断言：结果应为 false（未处理）。
        assert!(!result);
        // 断言：存储层的插入计数器应为 0。
        assert_eq!(storage.inserted.load(Ordering::SeqCst), 0);
    }

    // 测试用例：验证达到或超过阈值的支付是否被持久化。
    #[tokio::test]
    async fn persists_payments_at_threshold() {
        let storage = MockStorage::default();
        let min_payment_amount = 10;

        // 调用被测函数，传入金额为 10 的样本（等于阈值）。
        let result = process_entry(&storage, &sample_entry(10), min_payment_amount)
            .await
            .expect("processing succeeds");

        // 断言：结果应为 true（已处理）。
        assert!(result);
        // 断言：存储层的插入计数器应为 1。
        assert_eq!(storage.inserted.load(Ordering::SeqCst), 1);
    }
}