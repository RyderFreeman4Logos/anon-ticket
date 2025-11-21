use actix_web::{web, HttpResponse};
use anon_ticket_domain::model::{RevokeTokenRequest, ServiceToken};
use anon_ticket_domain::storage::TokenStore;
use chrono::{DateTime, Utc};
use metrics::counter;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

use super::ApiError;

#[derive(Debug, Serialize)]
pub struct TokenStatusResponse {
    pub status: String,
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
        "revoked"
    } else {
        "active"
    };
    counter!("api_token_requests_total", 1, "endpoint" => "status", "status" => status);
    Ok(HttpResponse::Ok().json(TokenStatusResponse {
        status: status.to_string(),
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
        return Err(ApiError::AlreadyRevoked);
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
        status: "revoked".to_string(),
        amount: updated.amount,
        issued_at: updated.issued_at,
        revoked_at: updated.revoked_at,
        abuse_score: updated.abuse_score,
    }))
}
