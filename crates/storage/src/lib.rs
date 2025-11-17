//! SeaORM-backed storage adapters that satisfy the domain storage traits while
//! keeping the database backend swappable (SQLite by default, PostgreSQL via
//! feature flag).

mod entity;

use std::sync::Arc;

use anon_ticket_domain::storage::{
    ClaimOutcome, MonitorStateStore, NewPayment, NewServiceToken, PaymentId, PaymentRecord,
    PaymentStatus, PaymentStore, RevokeTokenRequest, ServiceToken, ServiceTokenRecord,
    StorageError, StorageResult, TokenStore,
};
use chrono::Utc;
use entity::monitor_state;
use entity::payments::{self, PaymentStatusDb};
use entity::service_tokens;
use sea_orm::sea_query::{ColumnDef, Expr, OnConflict, Table, TableCreateStatement};
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseBackend,
    DatabaseConnection, EntityTrait, QueryFilter, Set, TransactionTrait,
};

/// Shared storage handle used by the HTTP API and monitor services.
#[derive(Clone)]
pub struct SeaOrmStorage {
    db: Arc<DatabaseConnection>,
}

impl SeaOrmStorage {
    /// Connects to the provided database URL and ensures the schema is present.
    pub async fn connect(database_url: &str) -> StorageResult<Self> {
        let db = Database::connect(database_url)
            .await
            .map_err(StorageError::from_source)?;
        run_migrations(&db).await?;

        Ok(Self { db: Arc::new(db) })
    }

    pub fn connection(&self) -> &DatabaseConnection {
        self.db.as_ref()
    }
}

async fn run_migrations(db: &DatabaseConnection) -> StorageResult<()> {
    let backend = db.get_database_backend();

    let payments_table = Table::create()
        .if_not_exists()
        .table(payments::Entity)
        .col(
            ColumnDef::new(payments::Column::Pid)
                .string()
                .not_null()
                .primary_key(),
        )
        .col(ColumnDef::new(payments::Column::Txid).string().not_null())
        .col(
            ColumnDef::new(payments::Column::Amount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(payments::Column::BlockHeight)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(payments::Column::Status)
                .string_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(payments::Column::CreatedAt)
                .date_time()
                .not_null()
                .default(Expr::current_timestamp()),
        )
        .col(
            ColumnDef::new(payments::Column::ClaimedAt)
                .date_time()
                .null(),
        )
        .to_owned();
    create_table(db, backend, payments_table).await?;

    let service_tokens_table = Table::create()
        .if_not_exists()
        .table(service_tokens::Entity)
        .col(
            ColumnDef::new(service_tokens::Column::Token)
                .string_len(128)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::Pid)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::Amount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::IssuedAt)
                .date_time()
                .not_null()
                .default(Expr::current_timestamp()),
        )
        .col(
            ColumnDef::new(service_tokens::Column::RevokedAt)
                .date_time()
                .null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::RevokeReason)
                .string()
                .null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::AbuseScore)
                .small_integer()
                .not_null()
                .default(0),
        )
        .to_owned();
    create_table(db, backend, service_tokens_table).await?;

    let monitor_table = Table::create()
        .if_not_exists()
        .table(monitor_state::Entity)
        .col(
            ColumnDef::new(monitor_state::Column::Key)
                .string_len(64)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(monitor_state::Column::ValueInt)
                .big_integer()
                .not_null(),
        )
        .to_owned();
    create_table(db, backend, monitor_table).await?;

    Ok(())
}

async fn create_table(
    db: &DatabaseConnection,
    backend: DatabaseBackend,
    mut statement: TableCreateStatement,
) -> StorageResult<()> {
    statement.if_not_exists();
    db.execute(backend.build(&statement))
        .await
        .map_err(StorageError::from_source)?;
    Ok(())
}

#[async_trait::async_trait]
impl PaymentStore for SeaOrmStorage {
    async fn insert_payment(&self, payment: NewPayment) -> StorageResult<()> {
        if payments::Entity::find()
            .filter(payments::Column::Pid.eq(payment.pid.as_str()))
            .one(self.connection())
            .await
            .map_err(StorageError::from_source)?
            .is_some()
        {
            return Ok(());
        }

        let model = payments::ActiveModel {
            pid: Set(payment.pid.into_inner()),
            txid: Set(payment.txid),
            amount: Set(payment.amount),
            block_height: Set(payment.block_height),
            status: Set(PaymentStatusDb::Unclaimed),
            created_at: Set(payment.detected_at),
            ..Default::default()
        };
        model
            .insert(self.connection())
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
                Expr::value(PaymentStatusDb::Claimed.to_value()),
            )
            .col_expr(payments::Column::ClaimedAt, Expr::value(now))
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

        Ok(Some(ClaimOutcome {
            pid: PaymentId::new(updated.pid),
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
        Ok(maybe.map(payment_to_record))
    }
}

#[async_trait::async_trait]
impl TokenStore for SeaOrmStorage {
    async fn insert_token(&self, token: NewServiceToken) -> StorageResult<ServiceTokenRecord> {
        let model = service_tokens::ActiveModel {
            token: Set(token.token.into_inner()),
            pid: Set(token.pid.into_inner()),
            amount: Set(token.amount),
            issued_at: Set(token.issued_at),
            abuse_score: Set(token.abuse_score),
            ..Default::default()
        };
        let created = model
            .insert(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(token_to_record(created))
    }

    async fn find_token(&self, token: &ServiceToken) -> StorageResult<Option<ServiceTokenRecord>> {
        let maybe = service_tokens::Entity::find()
            .filter(service_tokens::Column::Token.eq(token.as_str()))
            .one(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(maybe.map(token_to_record))
    }

    async fn revoke_token(
        &self,
        request: RevokeTokenRequest,
    ) -> StorageResult<Option<ServiceTokenRecord>> {
        let maybe = service_tokens::Entity::find()
            .filter(service_tokens::Column::Token.eq(request.token.as_str()))
            .one(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        let Some(model) = maybe else {
            return Ok(None);
        };

        if model.revoked_at.is_some() {
            return Ok(Some(token_to_record(model)));
        }

        let mut active: service_tokens::ActiveModel = model.into();
        active.revoked_at = Set(Some(Utc::now()));
        active.revoke_reason = Set(request.reason);
        if let Some(score) = request.abuse_score {
            active.abuse_score = Set(score);
        }
        let updated = active
            .update(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(Some(token_to_record(updated)))
    }
}

const LAST_HEIGHT_KEY: &str = "last_processed_height";

#[async_trait::async_trait]
impl MonitorStateStore for SeaOrmStorage {
    async fn last_processed_height(&self) -> StorageResult<Option<u64>> {
        let maybe = monitor_state::Entity::find_by_id(LAST_HEIGHT_KEY.to_string())
            .one(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(maybe.map(|model| model.value_int as u64))
    }

    async fn upsert_last_processed_height(&self, height: u64) -> StorageResult<()> {
        let active = monitor_state::ActiveModel {
            key: Set(LAST_HEIGHT_KEY.to_string()),
            value_int: Set(height as i64),
        };
        monitor_state::Entity::insert(active)
            .on_conflict(
                OnConflict::column(monitor_state::Column::Key)
                    .update_column(monitor_state::Column::ValueInt)
                    .to_owned(),
            )
            .exec(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        Ok(())
    }
}

fn payment_to_record(model: payments::Model) -> PaymentRecord {
    PaymentRecord {
        pid: PaymentId::new(model.pid),
        txid: model.txid,
        amount: model.amount,
        block_height: model.block_height,
        status: match model.status {
            PaymentStatusDb::Unclaimed => PaymentStatus::Unclaimed,
            PaymentStatusDb::Claimed => PaymentStatus::Claimed,
        },
        created_at: model.created_at,
        claimed_at: model.claimed_at,
    }
}

fn token_to_record(model: service_tokens::Model) -> ServiceTokenRecord {
    ServiceTokenRecord {
        token: ServiceToken::new(model.token),
        pid: PaymentId::new(model.pid),
        amount: model.amount,
        issued_at: model.issued_at,
        revoked_at: model.revoked_at,
        revoke_reason: model.revoke_reason,
        abuse_score: model.abuse_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anon_ticket_domain::storage::{NewPayment, NewServiceToken, RevokeTokenRequest};
    use chrono::Utc;

    fn test_pid() -> PaymentId {
        PaymentId::new("0123456789abcdef0123456789abcdef")
    }

    fn test_token() -> ServiceToken {
        ServiceToken::new("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
    }

    #[cfg(feature = "sqlite")]
    fn test_backend_url() -> Option<String> {
        Some("sqlite::memory:".to_string())
    }

    #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
    fn test_backend_url() -> Option<String> {
        std::env::var("TEST_POSTGRES_URL").ok()
    }

    #[cfg(not(any(feature = "sqlite", feature = "postgres")))]
    fn test_backend_url() -> Option<String> {
        None
    }

    async fn storage() -> Option<SeaOrmStorage> {
        let url = match test_backend_url() {
            Some(url) => url,
            None => return None,
        };

        Some(
            SeaOrmStorage::connect(&url)
                .await
                .unwrap_or_else(|err| panic!("failed to bootstrap test database `{url}`: {err}")),
        )
    }

    async fn storage_or_skip(test_name: &str) -> Option<SeaOrmStorage> {
        match storage().await {
            Some(store) => Some(store),
            None => {
                eprintln!(
                    "skipping {test_name}: no backend URL configured. For postgres tests set TEST_POSTGRES_URL."
                );
                None
            }
        }
    }

    #[tokio::test]
    async fn payment_lifecycle() {
        let Some(store) = storage_or_skip("payment_lifecycle").await else {
            return;
        };
        store
            .insert_payment(NewPayment {
                pid: test_pid(),
                txid: "tx1".into(),
                amount: 42,
                block_height: 100,
                detected_at: Utc::now(),
            })
            .await
            .unwrap();

        let claim = store.claim_payment(&test_pid()).await.unwrap();
        assert!(claim.is_some());
        let claim = claim.unwrap();
        assert_eq!(claim.amount, 42);
        assert_eq!(claim.block_height, 100);

        let second = store.claim_payment(&test_pid()).await.unwrap();
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn concurrent_claims_only_succeed_once() {
        let Some(store) = storage_or_skip("concurrent_claims_only_succeed_once").await else {
            return;
        };
        store
            .insert_payment(NewPayment {
                pid: test_pid(),
                txid: "tx1".into(),
                amount: 42,
                block_height: 100,
                detected_at: Utc::now(),
            })
            .await
            .unwrap();

        let store_a = store.clone();
        let store_b = store.clone();
        let pid = test_pid();
        let (first, second) =
            tokio::join!(store_a.claim_payment(&pid), store_b.claim_payment(&pid));

        let successes = [first.unwrap(), second.unwrap()]
            .into_iter()
            .filter(|outcome| outcome.is_some())
            .count();
        assert_eq!(successes, 1, "only one claimer should succeed");
    }

    #[tokio::test]
    async fn token_lifecycle() {
        let Some(store) = storage_or_skip("token_lifecycle").await else {
            return;
        };
        store
            .insert_payment(NewPayment {
                pid: test_pid(),
                txid: "tx1".into(),
                amount: 42,
                block_height: 100,
                detected_at: Utc::now(),
            })
            .await
            .unwrap();

        let token = store
            .insert_token(NewServiceToken {
                token: test_token(),
                pid: test_pid(),
                amount: 42,
                issued_at: Utc::now(),
                abuse_score: 0,
            })
            .await
            .unwrap();
        assert_eq!(token.pid.as_str(), test_pid().as_str());

        let revoked = store
            .revoke_token(RevokeTokenRequest {
                token: test_token(),
                reason: Some("abuse".into()),
                abuse_score: Some(5),
            })
            .await
            .unwrap()
            .expect("revoked");
        assert!(revoked.revoked_at.is_some());
        assert_eq!(revoked.abuse_score, 5);
    }

    #[tokio::test]
    async fn monitor_state_roundtrip() {
        let Some(store) = storage_or_skip("monitor_state_roundtrip").await else {
            return;
        };
        assert!(store.last_processed_height().await.unwrap().is_none());
        store.upsert_last_processed_height(1337).await.unwrap();
        assert_eq!(store.last_processed_height().await.unwrap(), Some(1337));
    }
}
