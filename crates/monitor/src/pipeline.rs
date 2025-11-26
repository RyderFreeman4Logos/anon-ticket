use anon_ticket_domain::model::{NewPayment, PaymentId};
use anon_ticket_domain::storage::PaymentStore;
use chrono::{DateTime, Utc};
use metrics::counter;
use tracing::warn;

use crate::rpc::TransferEntry;
use crate::worker::{MonitorError, MonitorHooks};

pub async fn process_entry<S>(
    storage: &S,
    entry: &TransferEntry,
    min_payment_amount: i64,
    hooks: Option<&MonitorHooks>,
) -> Result<bool, MonitorError>
where
    S: PaymentStore,
{
    let (Some(pid), Some(height)) = (&entry.payment_id, entry.height) else {
        return Ok(false);
    };

    if entry.amount < min_payment_amount {
        warn!(
            amount = entry.amount,
            min_payment_amount,
            txid = entry.txid,
            "skipping dust payment below minimum amount"
        );
        counter!(
            "monitor_payments_ingested_total",
            "result" => "dust"
        )
        .increment(1);
        return Ok(false);
    }

    let detected_at = DateTime::from_timestamp(entry.timestamp as i64, 0).unwrap_or_else(Utc::now);
    let pid = match PaymentId::parse(pid) {
        Ok(pid) => pid,
        Err(_) => {
            warn!(pid, "skipping invalid pid");
            counter!("monitor_payments_ingested_total", "result" => "invalid_pid").increment(1);
            return Ok(false);
        }
    };

    storage
        .insert_payment(NewPayment {
            pid: pid.clone(),
            txid: entry.txid.clone(),
            amount: entry.amount,
            block_height: height,
            detected_at,
        })
        .await?;
    if let Some(hooks) = hooks {
        hooks.mark_present(&pid);
    }
    counter!("monitor_payments_ingested_total", "result" => "persisted").increment(1);

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anon_ticket_domain::model::{ClaimOutcome, PaymentRecord};
    use anon_ticket_domain::storage::{PaymentStore, StorageResult};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct MockStorage {
        inserted: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl PaymentStore for MockStorage {
        async fn insert_payment(&self, _payment: NewPayment) -> StorageResult<()> {
            self.inserted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn claim_payment(&self, _pid: &PaymentId) -> StorageResult<Option<ClaimOutcome>> {
            Ok(None)
        }

        async fn find_payment(&self, _pid: &PaymentId) -> StorageResult<Option<PaymentRecord>> {
            Ok(None)
        }
    }

    fn sample_entry(amount: i64) -> TransferEntry {
        TransferEntry {
            txid: "tx1".to_string(),
            amount,
            height: Some(10),
            timestamp: 0,
            payment_id: Some("1111111111111111".to_string()),
        }
    }

    #[tokio::test]
    async fn skips_dust_below_threshold() {
        let storage = MockStorage::default();
        let min_payment_amount = 10;

        let result = process_entry(&storage, &sample_entry(5), min_payment_amount, None)
            .await
            .expect("processing succeeds");

        assert!(!result);
        assert_eq!(storage.inserted.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn persists_payments_at_threshold() {
        let storage = MockStorage::default();
        let min_payment_amount = 10;

        let result = process_entry(&storage, &sample_entry(10), min_payment_amount, None)
            .await
            .expect("processing succeeds");

        assert!(result);
        assert_eq!(storage.inserted.load(Ordering::SeqCst), 1);
    }
}
