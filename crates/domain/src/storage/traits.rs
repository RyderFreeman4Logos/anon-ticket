use async_trait::async_trait;
use thiserror::Error;

use crate::model::{
    ClaimOutcome, NewPayment, NewServiceToken, PaymentId, PaymentRecord, RevokeTokenRequest,
    ServiceToken, ServiceTokenRecord,
};

/// Common result alias for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(String),
}

impl StorageError {
    pub fn from_source(err: impl std::fmt::Display) -> Self {
        Self::Database(err.to_string())
    }
}

#[async_trait]
pub trait PaymentStore: Send + Sync {
    async fn insert_payment(&self, payment: NewPayment) -> StorageResult<()>;
    async fn claim_payment(&self, pid: &PaymentId) -> StorageResult<Option<ClaimOutcome>>;
    async fn find_payment(&self, pid: &PaymentId) -> StorageResult<Option<PaymentRecord>>;
}

#[async_trait]
pub trait TokenStore: Send + Sync {
    async fn insert_token(&self, token: NewServiceToken) -> StorageResult<ServiceTokenRecord>;
    async fn find_token(&self, token: &ServiceToken) -> StorageResult<Option<ServiceTokenRecord>>;
    async fn revoke_token(
        &self,
        request: RevokeTokenRequest,
    ) -> StorageResult<Option<ServiceTokenRecord>>;
}

#[async_trait]
pub trait MonitorStateStore: Send + Sync {
    async fn last_processed_height(&self) -> StorageResult<Option<u64>>;
    async fn upsert_last_processed_height(&self, height: u64) -> StorageResult<()>;
}
