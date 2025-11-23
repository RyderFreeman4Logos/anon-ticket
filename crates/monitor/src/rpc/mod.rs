use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Serialize;

use crate::worker::MonitorError;

mod types;

pub use types::{
    HeightResponse, JsonRpcRequest, JsonRpcResponse, TransferEntry, TransfersResponse,
};

#[async_trait]
pub trait TransferSource: Send + Sync {
    async fn fetch_transfers(&self, start_height: u64) -> Result<TransfersResponse, MonitorError>;
    async fn wallet_height(&self) -> Result<u64, MonitorError>;
}

pub struct RpcTransferSource {
    client: Client,
    rpc_url: String,
}

impl RpcTransferSource {
    pub fn new(client: Client, rpc_url: impl Into<String>) -> Self {
        Self {
            client,
            rpc_url: rpc_url.into(),
        }
    }
}

#[async_trait]
impl TransferSource for RpcTransferSource {
    async fn fetch_transfers(&self, start_height: u64) -> Result<TransfersResponse, MonitorError> {
        #[derive(Serialize)]
        struct Params {
            #[serde(rename = "in")]
            in_transfers: bool,
            out: bool,
            pending: bool,
            filter_by_height: bool,
            min_height: u64,
        }

        let params = Params {
            in_transfers: true,
            out: false,
            pending: false,
            filter_by_height: true,
            min_height: start_height,
        };

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "get_transfers".into(),
            params,
        };

        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;
        if resp.status() != StatusCode::OK {
            return Err(MonitorError::Rpc(format!("rpc failure {}", resp.status())));
        }

        let parsed: JsonRpcResponse<TransfersResponse> = resp.json().await?;
        Ok(parsed.result)
    }

    async fn wallet_height(&self) -> Result<u64, MonitorError> {
        #[derive(Serialize)]
        struct Params;

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "get_height".into(),
            params: Params,
        };

        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;
        if resp.status() != StatusCode::OK {
            return Err(MonitorError::Rpc(format!("rpc failure {}", resp.status())));
        }

        let parsed: JsonRpcResponse<HeightResponse> = resp.json().await?;
        Ok(parsed.result.height)
    }
}

#[cfg(test)]
mod tests {
    use super::TransfersResponse;

    #[test]
    fn deserializes_monero_rpc_response() {
        let json = r#"
        {
            "in": [
                {
                    "amount": 1000000000000,
                    "payment_id": "60900e56061d20a1",
                    "height": 123456,
                    "timestamp": 1535918400,
                    "txid": "c3d224630a6f59856302e592d329953df0b2a057693906976e5019df6347320d",
                    "type": "in",
                    "unlock_time": 0,
                    "double_spend_seen": false,
                    "fee": 0,
                    "note": "",
                    "subaddr_index": { "major": 0, "minor": 0 },
                    "confirmations": 1,
                    "suggested_confirmations_threshold": 1
                }
            ]
        }
        "#;

        let response: TransfersResponse = serde_json::from_str(json).expect("failed to parse json");
        assert_eq!(response.incoming.len(), 1);

        let entry = &response.incoming[0];
        assert_eq!(entry.amount, 1_000_000_000_000);
        assert_eq!(entry.payment_id.as_deref(), Some("60900e56061d20a1"));
        assert_eq!(entry.height, Some(123456));
    }
}
