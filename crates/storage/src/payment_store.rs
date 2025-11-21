use anon_ticket_domain::model::{
    ClaimOutcome, NewPayment, PaymentId, PaymentRecord, PaymentStatus,
};
use anon_ticket_domain::storage::{PaymentStore, StorageResult};
use chrono::Utc;
use sea_orm::sea_query::{PostgresQueryBuilder, Query, SqliteQueryBuilder};
use sea_orm::ActiveEnum;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, FromQueryResult, QueryFilter, Set,
    Statement,
};

use crate::entity::payments::{self, PaymentStatusDb};
use crate::errors::StorageError;
use crate::SeaOrmStorage;

#[async_trait::async_trait]
impl PaymentStore for SeaOrmStorage {
    async fn insert_payment(&self, payment: NewPayment) -> StorageResult<()> {
        let model = payments::ActiveModel {
            pid: Set(payment.pid.into_bytes().to_vec()),
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
        let now = Utc::now();
        let backend = self.connection().get_database_backend();

        let mut query = Query::update();
        query.table(payments::Entity);
        query.value(
            payments::Column::Status,
            PaymentStatusDb::Claimed.to_value(),
        );
        query.value(payments::Column::ClaimedAt, now);
        query.and_where(payments::Column::Pid.eq(pid.as_bytes().to_vec()));
        query.and_where(payments::Column::Status.eq(PaymentStatusDb::Unclaimed));
        query.returning_all();

        let (sql, values) = match backend {
            DatabaseBackend::Sqlite => query.build(SqliteQueryBuilder),
            DatabaseBackend::Postgres => query.build(PostgresQueryBuilder),
            DatabaseBackend::MySql => unreachable!("mysql backend is not supported"),
        };
        let stmt = Statement::from_sql_and_values(backend, sql, values);
        let maybe_row = self
            .connection()
            .query_one(stmt)
            .await
            .map_err(StorageError::from_source)?;

        let updated = match maybe_row {
            Some(row) => {
                payments::Model::from_query_result(&row, "").map_err(StorageError::from_source)?
            }
            None => return Ok(None),
        };

        let pid = PaymentId::try_from(updated.pid)
            .map_err(|err| StorageError::Database(err.to_string()))?;

        Ok(Some(ClaimOutcome {
            pid,
            txid: updated.txid,
            amount: updated.amount,
            block_height: updated.block_height,
            claimed_at: updated.claimed_at.unwrap_or(now),
        }))
    }

    async fn find_payment(&self, pid: &PaymentId) -> StorageResult<Option<PaymentRecord>> {
        let maybe = payments::Entity::find()
            .filter(payments::Column::Pid.eq(pid.as_bytes().to_vec()))
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
