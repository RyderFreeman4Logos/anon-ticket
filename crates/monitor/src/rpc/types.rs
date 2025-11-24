#[derive(Debug, Clone, Default)]
pub struct TransfersResponse {
    pub incoming: Vec<TransferEntry>,
}

#[derive(Debug, Clone)]
pub struct TransferEntry {
    pub txid: String,
    /// Amount in atomic units.
    pub amount: i64,
    pub height: Option<i64>,
    pub timestamp: u64,
    pub payment_id: Option<String>,
}
