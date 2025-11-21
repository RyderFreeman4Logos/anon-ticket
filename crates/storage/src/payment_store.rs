use anon_ticket_domain::model::{
    ClaimOutcome, NewPayment, PaymentId, PaymentRecord, PaymentStatus,
};
use anon_ticket_domain::storage::{PaymentStore, StorageResult};
use chrono::Utc;
use sea_orm::ActiveEnum;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait};

use crate::entity::payments::{self, PaymentStatusDb};
use crate::errors::StorageError;
use crate::SeaOrmStorage;

#[async_trait::async_trait]
impl PaymentStore for SeaOrmStorage {
    async fn insert_payment(&self, payment: NewPayment) -> StorageResult<()> {
        let model = payments::ActiveModel {
            pid: Set(payment.pid.into_inner()),
            txid: Set(payment.txid),
            amount: Set(payment.amount),
            block_height: Set(payment.block_height),
            status: Set(PaymentStatusDb::Unclaimed),
            created_at: Set(payment.detected_at),
            ..Default::default()
        };
        payments::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(payments::Column::Pid)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(())
    }

    async fn claim_payment(&self, pid: &PaymentId) -> StorageResult<Option<ClaimOutcome>> {
        let txn = self
            .connection()
            .begin()
            .await
            .map_err(StorageError::from_source)?;
        let now = Utc::now();
        let update_result = payments::Entity::update_many()
            .col_expr(
                payments::Column::Status,
                sea_orm::sea_query::Expr::value(PaymentStatusDb::Claimed.to_value()),
            )
            .col_expr(
                payments::Column::ClaimedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(payments::Column::Pid.eq(pid.as_str()))
            .filter(payments::Column::Status.eq(PaymentStatusDb::Unclaimed))
            .exec(&txn)
            .await
            .map_err(StorageError::from_source)?;

        if update_result.rows_affected == 0 {
            txn.commit().await.map_err(StorageError::from_source)?;
            return Ok(None);
        }

        let updated = payments::Entity::find()
            .filter(payments::Column::Pid.eq(pid.as_str()))
            .one(&txn)
            .await
            .map_err(StorageError::from_source)?
            .ok_or_else(|| StorageError::Database("claimed payment missing".to_string()))?;

        txn.commit().await.map_err(StorageError::from_source)?;

        let pid = PaymentId::try_from(updated.pid)
            .map_err(|err| StorageError::Database(err.to_string()))?;

        Ok(Some(ClaimOutcome {
            pid,
            txid: updated.txid,
            amount: updated.amount,
            block_height: updated.block_height,
            claimed_at: updated.claimed_at.expect("claimed timestamp set"),
        }))
    }

    async fn find_payment(&self, pid: &PaymentId) -> StorageResult<Option<PaymentRecord>> {
        let maybe = payments::Entity::find()
            .filter(payments::Column::Pid.eq(pid.as_str()))
            .one(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        maybe.map(payment_to_record).transpose()
    }
}

fn payment_to_record(model: payments::Model) -> StorageResult<PaymentRecord> {
    let pid =
        PaymentId::try_from(model.pid).map_err(|err| StorageError::Database(err.to_string()))?;

    Ok(PaymentRecord {
        txid: model.txid,
        amount: model.amount,
        block_height: model.block_height,
        status: match model.status {
            PaymentStatusDb::Unclaimed => PaymentStatus::Unclaimed,
            PaymentStatusDb::Claimed => PaymentStatus::Claimed,
        },
        created_at: model.created_at,
        claimed_at: model.claimed_at,
        pid,
    })
}
