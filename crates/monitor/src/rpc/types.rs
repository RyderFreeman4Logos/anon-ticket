use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    pub params: T,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub result: T,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct TransfersResponse {
    #[serde(default, rename = "in")]
    pub incoming: Vec<TransferEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TransferEntry {
    pub txid: String,
    /// Amount in atomic units.
    /// Note: Uses `i64` to match SQLite storage compatibility.
    /// Max value is ~9.22e18 (9.22 million XMR), which is safe for individual payments
    /// but technically smaller than the total supply.
    pub amount: i64,
    pub height: Option<i64>,
    pub timestamp: u64,
    #[serde(default)]
    pub payment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HeightResponse {
    pub height: u64,
}
