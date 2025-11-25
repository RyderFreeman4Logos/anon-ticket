// 引入标准库中的 HashMap，用于存储键值对集合。在这里主要用于配置 RPC 请求的参数。
use std::collections::HashMap;

// 引入当前 crate 中 `worker` 模块定义的 `MonitorError` 枚举，用于统一处理错误。
use crate::worker::MonitorError;
// 引入领域模型中的 `PaymentId` 类型，用于处理 Monero 的支付 ID。
use anon_ticket_domain::model::PaymentId;
// 引入 `async_trait` 宏。Rust 的原生 trait 目前还不支持异步函数，所以需要这个库来简化异步 trait 的定义和实现。
use async_trait::async_trait;

// 引入 `monero_rpc` crate 中的相关类型，用于与 Monero 钱包 RPC 接口进行交互。
// 包括区块高度过滤器、转账类别、选择器、转账高度枚举和钱包客户端。
use monero_rpc::{
    BlockHeightFilter, GetTransfersCategory, GetTransfersSelector, TransferHeight, WalletClient,
};

// 声明并引入 `types` 子模块，该模块定义了数据传输对象（DTO）。
mod types;

// 重新导出 `types` 模块中的 `TransferEntry` 和 `TransfersResponse`，方便外部直接使用。
pub use types::{TransferEntry, TransfersResponse};

// 定义 `TransferSource` trait。这是一个抽象接口，定义了获取转账记录的能力。
// `Send + Sync` 是 Rust 并发编程的标记 trait，确保实现该 trait 的对象可以在线程间安全地传递和共享。
#[async_trait]
pub trait TransferSource: Send + Sync {
    // 异步方法 `fetch_transfers`：从指定的高度开始获取转账记录。
    // 参数 `start_height`: 搜索的起始区块高度。
    // 返回值: `Result<TransfersResponse, MonitorError>`，成功时返回转账响应，失败时返回监控错误。
    async fn fetch_transfers(&self, start_height: u64) -> Result<TransfersResponse, MonitorError>;

    // 异步方法 `wallet_height`：获取当前钱包同步到的最高区块高度。
    // 这通常用于确定下一次扫描的起始位置或检查同步进度。
    async fn wallet_height(&self) -> Result<u64, MonitorError>;
}

// 定义 `RpcTransferSource` 结构体，它是 `TransferSource` trait 的具体实现。
// 它持有一个 `WalletClient` 实例，通过 JSON-RPC 与 Monero 钱包进行通信。
pub struct RpcTransferSource {
    wallet: WalletClient,
}

// `RpcTransferSource` 的实现块。
impl RpcTransferSource {
    // 构造函数：创建一个新的 `RpcTransferSource` 实例。
    // 接收一个已经配置好的 `WalletClient`。
    pub fn new(wallet: WalletClient) -> Self {
        Self { wallet }
    }
}

// 为 `RpcTransferSource` 实现 `TransferSource` trait。
// `#[async_trait]` 宏会自动处理异步函数的生命周期和返回值包装。
#[async_trait]
impl TransferSource for RpcTransferSource {
    // 实现 `fetch_transfers` 方法，具体逻辑如下：
    async fn fetch_transfers(&self, start_height: u64) -> Result<TransfersResponse, MonitorError> {
        // 创建一个 HashMap 来配置要获取的转账类别。
        // 这里只关心 `GetTransfersCategory::In`（也就是“传入”的转账/收款）。
        let mut categories = HashMap::new();
        categories.insert(GetTransfersCategory::In, true);

        // 构建 `GetTransfersSelector` 选择器结构体，用于通过 RPC 筛选转账。
        let selector = GetTransfersSelector {
            category_selector: categories, // 设置类别过滤器
            account_index: None,          // None 表示扫描所有账户索引
            subaddr_indices: None,        // None 表示扫描所有子地址索引
            block_height_filter: Some(BlockHeightFilter {
                // 设置最小区块高度过滤器，只获取 `start_height` 之后的交易。
                min_height: Some(start_height),
                // max_height 为 None 表示直到最新区块。
                max_height: None,
            }),
        };

        // 调用钱包客户端的 `get_transfers` 方法发送 RPC 请求。
        // `.await` 等待异步操作完成。
        // `map_err` 将 RPC 产生的错误转换为我们自定义的 `MonitorError::Rpc` 错误类型。
        let mut result = self
            .wallet
            .get_transfers(selector)
            .await
            .map_err(|err| MonitorError::Rpc(err.to_string()))?;

        // 从结果中提取“传入”类别的转账列表。
        // 如果没有找到该类别的记录，则默认为空列表。
        let incoming = result.remove(&GetTransfersCategory::In).unwrap_or_default();

        // 预分配一个 vector 来存储转换后的转账条目，提高性能。
        let mut entries = Vec::with_capacity(incoming.len());
        // 遍历每一条原始转账记录，将其转换为内部使用的 `TransferEntry` 格式。
        for transfer in incoming {
            // 调用 `convert_transfer` 辅助函数进行转换。
            // 如果转换成功且返回 Some（表示有效转账），则加入列表。
            if let Some(entry) = convert_transfer(transfer)? {
                entries.push(entry);
            }
        }

        // 返回最终封装好的 `TransfersResponse`。
        Ok(TransfersResponse { incoming: entries })
    }

    // 实现 `wallet_height` 方法，获取钱包当前的区块高度。
    async fn wallet_height(&self) -> Result<u64, MonitorError> {
        Ok(self
            .wallet
            .get_height() // 调用 RPC 获取高度
            .await
            .map_err(|err| MonitorError::Rpc(err.to_string()))? // 错误处理
            .get()) // 解包获取具体的 u64 高度值
    }
}

// 辅助函数：将 `monero_rpc` 库返回的 `GotTransfer` 结构体转换为我们自定义的 `TransferEntry`。
// 返回值是 `Result<Option<TransferEntry>, MonitorError>`：
// - `Ok(Some(...))` 表示转换成功且数据有效。
// - `Ok(None)` 表示数据无效或不符合要求（例如解析 PaymentId 失败），可以忽略。
// - `Err(...)` 表示发生了严重错误（如数值溢出）。
fn convert_transfer(
    transfer: monero_rpc::GotTransfer,
) -> Result<Option<TransferEntry>, MonitorError> {
    // 将金额从 Monero 的特殊类型转换为 `i64`。
    // 如果数值过大导致 `i64` 溢出，则返回错误。这是为了确保数据在系统内的安全性。
    let amount = i64::try_from(transfer.amount.as_pico())
        .map_err(|_| MonitorError::Rpc("amount overflow".to_string()))?;

    // 处理区块高度。
    // `TransferHeight` 枚举可能是 `Confirmed`（已确认，包含高度）或 `InPool`（在内存池中，无高度）。
    let height = match transfer.height {
        TransferHeight::Confirmed(h) => Some(h.get() as i64),
        TransferHeight::InPool => None,
    };

    // 获取 Payment ID 的十六进制字符串表示。
    let payment_id_hex = transfer.payment_id.to_string();
    // 尝试将其解析为领域模型中的 `PaymentId` 类型。
    // 如果解析成功，说明是一个有效的 Payment ID。
    // 如果解析失败，返回 `None`，意味着这笔交易可能没有 Payment ID 或者格式不正确，我们选择忽略它（返回 `Ok(None)` 而不是 Err）。
    let payment_id = match PaymentId::parse(&payment_id_hex) {
        Ok(_) => Some(payment_id_hex),
        Err(_) => None,
    };

    // 将时间戳转换为 u64 类型。
    let timestamp = transfer.timestamp.timestamp() as u64;

    // 构建并返回 `TransferEntry` 对象。
    Ok(Some(TransferEntry {
        txid: transfer.txid.to_string(), // 交易哈希
        amount,
        height,
        timestamp,
        payment_id,
    }))
}

// 单元测试模块，用于验证代码逻辑的正确性。
#[cfg(test)]
mod tests {
    use super::*; // 引入外部模块的所有内容
    // 引入测试所需的依赖类型
    use monero_rpc::{
        monero::{
            cryptonote::subaddress, util::address::PaymentId as RpcPaymentId, Address, Amount,
        },
        HashString, TransferHeight,
    };
    use std::num::NonZeroU64;
    use std::str::FromStr;

    // 测试用例：验证 `convert_transfer` 函数能否正确转换数据。
    #[test]
    fn converts_got_transfer_into_entry() {
        // 构造一个模拟的 Monero 地址
        let address = Address::from_str(
            "4ADT1BtbxqEWeMKp9GgPr2NeyJXXtNxvoDawpyA4WpzFcGcoHUvXeijE66DNfohE9r1bQYaBiQjEtKE7CtkTdLwiDznFzra",
        )
        .unwrap();
        // 构造一个模拟的 Payment ID
        let payment_id = RpcPaymentId::from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
        // 构造一个模拟的交易 ID (TxID)
        let txid = HashString::<Vec<u8>>(
            hex::decode("c3d224630a6f59856302e592d329953df0b2a057693906976e5019df6347320d")
                .unwrap(),
        );

        // 手动构建一个 `monero_rpc::GotTransfer` 对象，模拟 RPC 返回的数据。
        let transfer = monero_rpc::GotTransfer {
            address,
            amount: Amount::from_pico(1_000_000), // 1,000,000 atomic units
            confirmations: Some(1),
            double_spend_seen: false,
            fee: Amount::from_pico(0),
            height: TransferHeight::Confirmed(NonZeroU64::new(123456).unwrap()), // 高度 123456
            note: String::new(),
            destinations: None,
            payment_id: HashString(payment_id),
            subaddr_index: subaddress::Index { major: 0, minor: 0 },
            suggested_confirmations_threshold: Some(1),
            timestamp: chrono::Utc::now(),
            txid,
            transfer_type: GetTransfersCategory::In, // 类型为“传入”
            unlock_time: 0,
        };

        // 调用被测函数
        let entry = convert_transfer(transfer)
            .expect("conversion succeeds") // 期望转换不报错
            .expect("entry present");      // 期望返回 Some(entry)

        // 断言：验证转换后的字段值是否符合预期
        assert_eq!(entry.amount, 1_000_000);
        assert_eq!(entry.height, Some(123456));
        // Payment ID 0001020304050607 对应的十六进制字符串
        assert_eq!(entry.payment_id.as_deref(), Some("0001020304050607"));
    }
}
