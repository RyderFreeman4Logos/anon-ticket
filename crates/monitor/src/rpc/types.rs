// 这是一个 Rust 结构体定义的模块，用于定义 RPC（远程过程调用）传输层的数据类型。
// 这些结构体主要用于在应用程序内部传递从 Monero 钱包 RPC 获取的转账信息。

// 使用 `derive` 宏自动为结构体实现 `Debug`（用于调试打印）、`Clone`（用于复制对象）和 `Default`（提供默认值）trait。
#[derive(Debug, Clone, Default)]
// 定义 `TransfersResponse` 结构体，用于封装从 RPC 获取的一批转账记录。
pub struct TransfersResponse {
    // `incoming` 字段是一个 `Vec`（动态数组），存储了多个 `TransferEntry` 对象。
    // 这代表了所有“传入”的转账记录列表。
    pub incoming: Vec<TransferEntry>,
}

// 使用 `derive` 宏为结构体实现 `Debug` 和 `Clone` trait。
// 注意：这里没有 `Default`，这意味着创建此结构体时必须提供所有字段的值。
#[derive(Debug, Clone)]
// 定义 `TransferEntry` 结构体，代表单笔转账的具体细节。
// 这个结构体是业务逻辑中处理的核心数据单元。
pub struct TransferEntry {
    // `txid` (Transaction ID) 是交易的哈希值，作为字符串存储。
    // 它是区块链上每一笔交易的唯一标识符。
    pub txid: String,

    // `amount` 表示交易金额。
    // 这里使用 `i64` 类型，单位通常是 atomic units（原子单位，例如 Monero 的 piconero）。
    // 使用整数而非浮点数是为了避免精度丢失问题，这在金融计算中至关重要。
    /// Amount in atomic units.
    pub amount: i64,

    // `height` 表示包含该交易的区块高度。
    // `Option<i64>` 表示这个值可能是 `None`（例如，交易还在内存池中，尚未打包进区块）。
    pub height: Option<i64>,

    // `timestamp` 是交易发生的时间戳，通常是 Unix 时间戳（秒）。
    // 使用 `u64` 存储非负的大整数。
    pub timestamp: u64,

    // `payment_id` 是 Monero 特有的概念，用于区分同一地址下的不同支付。
    // `Option<String>` 表示这个支付 ID 是可选的。
    // 在旧版 Monero 协议中常用于关联订单，现在更推荐使用集成地址（Integrated Address）。
    pub payment_id: Option<String>,
}