use anon_ticket_domain::model::{
    NewServiceToken, PaymentId, RevokeTokenRequest, ServiceToken, ServiceTokenRecord,
};
use anon_ticket_domain::storage::{StorageResult, TokenStore};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::entity::service_tokens;
use crate::errors::StorageError;
use crate::SeaOrmStorage;

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
        token_to_record(created)
    }

    async fn find_token(&self, token: &ServiceToken) -> StorageResult<Option<ServiceTokenRecord>> {
        let maybe = service_tokens::Entity::find()
            .filter(service_tokens::Column::Token.eq(token.as_str()))
            .one(self.connection())
            .await
            .map_err(StorageError::from_source)?;
        maybe.map(token_to_record).transpose()
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
            return token_to_record(model).map(Some);
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
        token_to_record(updated).map(Some)
    }
}

fn token_to_record(model: service_tokens::Model) -> StorageResult<ServiceTokenRecord> {
    let pid =
        PaymentId::try_from(model.pid).map_err(|err| StorageError::Database(err.to_string()))?;

    Ok(ServiceTokenRecord {
        token: ServiceToken::new(model.token),
        pid,
        amount: model.amount,
        issued_at: model.issued_at,
        revoked_at: model.revoked_at,
        revoke_reason: model.revoke_reason,
        abuse_score: model.abuse_score,
    })
}
