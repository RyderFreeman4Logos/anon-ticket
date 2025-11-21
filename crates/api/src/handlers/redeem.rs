use std::time::Duration;

use actix_web::{web, HttpResponse};
use anon_ticket_domain::model::{
    derive_service_token, ClaimOutcome, NewServiceToken, PaymentId, PaymentRecord, PaymentStatus,
    ServiceTokenRecord,
};
use anon_ticket_domain::storage::{PaymentStore, TokenStore};
use anon_ticket_domain::PidCache;
use chrono::Utc;
use metrics::counter;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

use super::ApiError;

pub const PID_CACHE_NEGATIVE_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Deserialize, Serialize)]
pub struct RedeemRequest {
    pub pid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RedeemResponse {
    pub status: String,
    pub service_token: String,
    pub balance: i64,
}

pub async fn redeem_handler(
    state: web::Data<AppState>,
    payload: web::Json<RedeemRequest>,
) -> Result<HttpResponse, ApiError> {
    let pid = PaymentId::parse(&payload.pid).inspect_err(|_| {
        counter!("api_redeem_requests_total", 1, "status" => "invalid_pid");
    })?;

    if !state.cache().might_contain(&pid) {
        let should_short_circuit = state
            .cache()
            .negative_entry_age(&pid)
            .is_some_and(|age| age < PID_CACHE_NEGATIVE_GRACE);
        if should_short_circuit {
            counter!("api_redeem_cache_hints_total", 1, "hint" => "absent_blocked");
            counter!("api_redeem_requests_total", 1, "status" => "cache_absent");
            return Err(ApiError::NotFound);
        }

        counter!("api_redeem_cache_hints_total", 1, "hint" => "absent_probe");
    }

    match state.storage().claim_payment(&pid).await? {
        Some(outcome) => handle_success(&state, pid, outcome).await,
        None => handle_absent(&state, pid).await,
    }
}

async fn handle_success(
    state: &AppState,
    pid: PaymentId,
    outcome: ClaimOutcome,
) -> Result<HttpResponse, ApiError> {
    let service_token = derive_service_token(&pid, &outcome.txid);
    let token_record = state
        .storage()
        .insert_token(NewServiceToken {
            token: service_token,
            pid: pid.clone(),
            amount: outcome.amount,
            issued_at: outcome.claimed_at,
            abuse_score: 0,
        })
        .await?;
    counter!("api_redeem_requests_total", 1, "status" => "success");
    state.cache().mark_present(&pid);

    Ok(HttpResponse::Ok().json(build_redeem_response("success", token_record)))
}

async fn handle_absent(state: &AppState, pid: PaymentId) -> Result<HttpResponse, ApiError> {
    let maybe_payment = state.storage().find_payment(&pid).await?;
    match maybe_payment {
        Some(record) if record.status == PaymentStatus::Claimed => {
            state.cache().mark_present(&pid);
            let token = ensure_token_record(state, &pid, &record).await?;
            counter!("api_redeem_requests_total", 1, "status" => "already_claimed");
            Ok(HttpResponse::Ok().json(build_redeem_response("already_claimed", token)))
        }
        Some(_) => {
            state.cache().mark_present(&pid);
            counter!("api_redeem_requests_total", 1, "status" => "pending");
            Err(ApiError::NotFound)
        }
        None => {
            state.cache().mark_absent(&pid);
            counter!("api_redeem_requests_total", 1, "status" => "not_found");
            Err(ApiError::NotFound)
        }
    }
}

fn build_redeem_response(status: &str, record: ServiceTokenRecord) -> RedeemResponse {
    RedeemResponse {
        status: status.to_string(),
        service_token: record.token.into_inner(),
        balance: record.amount,
    }
}

async fn ensure_token_record(
    state: &AppState,
    pid: &PaymentId,
    payment: &PaymentRecord,
) -> Result<ServiceTokenRecord, ApiError> {
    let token = derive_service_token(pid, &payment.txid);
    if let Some(existing) = state.storage().find_token(&token).await? {
        return Ok(existing);
    }
    let issued_at = payment.claimed_at.unwrap_or_else(Utc::now);
    match state
        .storage()
        .insert_token(NewServiceToken {
            token: token.clone(),
            pid: pid.clone(),
            amount: payment.amount,
            issued_at,
            abuse_score: 0,
        })
        .await
        .map_err(ApiError::from)
    {
        Ok(record) => Ok(record),
        Err(ApiError::Storage(err)) if err.to_string().to_lowercase().contains("unique") => state
            .storage()
            .find_token(&token)
            .await?
            .ok_or(ApiError::NotFound),
        Err(other) => Err(other),
    }
}
