use actix_web::{web, HttpResponse};
use anon_ticket_domain::model::{RevokeTokenRequest, ServiceToken};
use anon_ticket_domain::storage::TokenStore;
use chrono::{DateTime, Utc};
use metrics::counter;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;

use crate::state::AppState;

use super::ApiError;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, AsRefStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TokenState {
    Active,
    Revoked,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenStatusResponse {
    pub status: TokenState,
    pub amount: i64,
    pub issued_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub abuse_score: i16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RevokeRequest {
    pub reason: Option<String>,
    pub abuse_score: Option<i16>,
}

pub async fn token_status_handler(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let token = ServiceToken::parse(&path.into_inner())?;
    let record = match state.storage().find_token(&token).await? {
        Some(record) => record,
        None => {
            counter!("api_token_requests_total", 1, "endpoint" => "status", "status" => "not_found");
            return Err(ApiError::NotFound);
        }
    };
    let status = if record.revoked_at.is_some() {
        TokenState::Revoked
    } else {
        TokenState::Active
    };
    let status_tag = status.as_ref().to_owned();
    counter!("api_token_requests_total", 1, "endpoint" => "status", "status" => status_tag);
    Ok(HttpResponse::Ok().json(TokenStatusResponse {
        status,
        amount: record.amount,
        issued_at: record.issued_at,
        revoked_at: record.revoked_at,
        abuse_score: record.abuse_score,
    }))
}

pub async fn revoke_token_handler(
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<RevokeRequest>,
) -> Result<HttpResponse, ApiError> {
    let token = ServiceToken::parse(&path.into_inner())?;
    let existing = match state.storage().find_token(&token).await? {
        Some(record) => record,
        None => {
            counter!("api_token_requests_total", 1, "endpoint" => "revoke", "status" => "not_found");
            return Err(ApiError::NotFound);
        }
    };
    if existing.revoked_at.is_some() {
        counter!("api_token_requests_total", 1, "endpoint" => "revoke", "status" => "already_revoked");
        return Ok(HttpResponse::Ok().json(TokenStatusResponse {
            status: TokenState::Revoked,
            amount: existing.amount,
            issued_at: existing.issued_at,
            revoked_at: existing.revoked_at,
            abuse_score: existing.abuse_score,
        }));
    }
    let updated = state
        .storage()
        .revoke_token(RevokeTokenRequest {
            token,
            reason: payload.reason.clone(),
            abuse_score: payload.abuse_score,
        })
        .await?
        .ok_or(ApiError::NotFound)?;
    counter!("api_token_requests_total", 1, "endpoint" => "revoke", "status" => "revoked");
    Ok(HttpResponse::Ok().json(TokenStatusResponse {
        status: TokenState::Revoked,
        amount: updated.amount,
        issued_at: updated.issued_at,
        revoked_at: updated.revoked_at,
        abuse_score: updated.abuse_score,
    }))
}
