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

#[derive(Debug, Deserialize, Default)]
pub struct TransfersResponse {
    #[serde(default)]
    pub out: Vec<TransferEntry>,
}

#[derive(Debug, Deserialize)]
pub struct TransferEntry {
    pub txid: String,
    pub amount: i64,
    pub height: Option<i64>,
    pub timestamp: u64,
    #[serde(default)]
    pub payment_id: Option<String>,
}
