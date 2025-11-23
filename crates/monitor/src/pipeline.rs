use anon_ticket_domain::model::{NewPayment, PaymentId};
use anon_ticket_domain::storage::PaymentStore;
use chrono::{DateTime, Utc};
use metrics::counter;
use tracing::warn;

use crate::rpc::TransferEntry;
use crate::worker::MonitorError;

pub async fn process_entry<S>(storage: &S, entry: &TransferEntry) -> Result<bool, MonitorError>
where
    S: PaymentStore,
{
    let (Some(pid), Some(height)) = (&entry.payment_id, entry.height) else {
        return Ok(false);
    };

    let detected_at = DateTime::from_timestamp(entry.timestamp as i64, 0).unwrap_or_else(Utc::now);
    let pid = match PaymentId::parse(pid) {
        Ok(pid) => pid,
        Err(_) => {
            warn!(pid, "skipping invalid pid");
            counter!("monitor_payments_ingested_total", 1, "result" => "invalid_pid");
            return Ok(false);
        }
    };

    storage
        .insert_payment(NewPayment {
            pid,
            txid: entry.txid.clone(),
            amount: entry.amount,
            block_height: height,
            detected_at,
        })
        .await?;
    counter!("monitor_payments_ingested_total", 1, "result" => "persisted");

    Ok(true)
}
