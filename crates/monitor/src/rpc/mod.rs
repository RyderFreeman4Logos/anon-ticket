use std::collections::HashMap;

use crate::worker::MonitorError;
use anon_ticket_domain::model::PaymentId;
use async_trait::async_trait;

use monero_rpc::{
    BlockHeightFilter, GetTransfersCategory, GetTransfersSelector, TransferHeight, WalletClient,
};

mod types;

pub use types::{TransferEntry, TransfersResponse};

#[async_trait]
pub trait TransferSource: Send + Sync {
    async fn fetch_transfers(&self, start_height: u64) -> Result<TransfersResponse, MonitorError>;
    async fn wallet_height(&self) -> Result<u64, MonitorError>;
}

pub struct RpcTransferSource {
    wallet: WalletClient,
}

impl RpcTransferSource {
    pub fn new(wallet: WalletClient) -> Self {
        Self { wallet }
    }
}

#[async_trait]
impl TransferSource for RpcTransferSource {
    async fn fetch_transfers(&self, start_height: u64) -> Result<TransfersResponse, MonitorError> {
        let mut categories = HashMap::new();
        categories.insert(GetTransfersCategory::In, true);

        let selector = GetTransfersSelector {
            category_selector: categories,
            account_index: None,
            subaddr_indices: None,
            block_height_filter: Some(BlockHeightFilter {
                min_height: Some(start_height),
                max_height: None,
            }),
        };

        let mut result = self
            .wallet
            .get_transfers(selector)
            .await
            .map_err(|err| MonitorError::Rpc(err.to_string()))?;

        let incoming = result.remove(&GetTransfersCategory::In).unwrap_or_default();

        let mut entries = Vec::with_capacity(incoming.len());
        for transfer in incoming {
            if let Some(entry) = convert_transfer(transfer)? {
                entries.push(entry);
            }
        }

        Ok(TransfersResponse { incoming: entries })
    }

    async fn wallet_height(&self) -> Result<u64, MonitorError> {
        Ok(self
            .wallet
            .get_height()
            .await
            .map_err(|err| MonitorError::Rpc(err.to_string()))?
            .get())
    }
}

fn convert_transfer(
    transfer: monero_rpc::GotTransfer,
) -> Result<Option<TransferEntry>, MonitorError> {
    let amount = i64::try_from(transfer.amount.as_pico())
        .map_err(|_| MonitorError::Rpc("amount overflow".to_string()))?;

    let height = match transfer.height {
        TransferHeight::Confirmed(h) => Some(h.get() as i64),
        TransferHeight::InPool => None,
    };

    let payment_id_hex = transfer.payment_id.to_string();
    let payment_id = match PaymentId::parse(&payment_id_hex) {
        Ok(_) => Some(payment_id_hex),
        Err(_) => None,
    };

    let timestamp = transfer.timestamp.timestamp() as u64;

    Ok(Some(TransferEntry {
        txid: transfer.txid.to_string(),
        amount,
        height,
        timestamp,
        payment_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use monero_rpc::{
        monero::{
            cryptonote::subaddress, util::address::PaymentId as RpcPaymentId, Address, Amount,
        },
        HashString, TransferHeight,
    };
    use std::num::NonZeroU64;
    use std::str::FromStr;

    #[test]
    fn converts_got_transfer_into_entry() {
        let address = Address::from_str(
            "4ADT1BtbxqEWeMKp9GgPr2NeyJXXtNxvoDawpyA4WpzFcGcoHUvXeijE66DNfohE9r1bQYaBiQjEtKE7CtkTdLwiDznFzra",
        )
        .unwrap();
        let payment_id = RpcPaymentId::from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
        let txid = HashString::<Vec<u8>>(
            hex::decode("c3d224630a6f59856302e592d329953df0b2a057693906976e5019df6347320d")
                .unwrap(),
        );

        let transfer = monero_rpc::GotTransfer {
            address,
            amount: Amount::from_pico(1_000_000),
            confirmations: Some(1),
            double_spend_seen: false,
            fee: Amount::from_pico(0),
            height: TransferHeight::Confirmed(NonZeroU64::new(123456).unwrap()),
            note: String::new(),
            destinations: None,
            payment_id: HashString(payment_id),
            subaddr_index: subaddress::Index { major: 0, minor: 0 },
            suggested_confirmations_threshold: Some(1),
            timestamp: chrono::Utc::now(),
            txid,
            transfer_type: GetTransfersCategory::In,
            unlock_time: 0,
        };

        let entry = convert_transfer(transfer)
            .expect("conversion succeeds")
            .expect("entry present");

        assert_eq!(entry.amount, 1_000_000);
        assert_eq!(entry.height, Some(123456));
        assert_eq!(entry.payment_id.as_deref(), Some("0001020304050607"));
    }
}
