//! Actix-Web API exposing the redemption endpoint.

use std::{io, sync::Arc};

use actix_web::{
    http::StatusCode,
    middleware::Logger,
    web::{self, Data},
    App, HttpResponse, HttpServer, ResponseError,
};
use anon_ticket_domain::{
    derive_service_token,
    storage::{
        ClaimOutcome, NewServiceToken, PaymentStatus, PaymentStore, RevokeTokenRequest,
        ServiceToken, TokenStore,
    },
    ApiConfig, ConfigError, InMemoryPidCache, PaymentId, PidCache, PidFormatError, StorageError,
};
use anon_ticket_storage::SeaOrmStorage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone)]
struct AppState {
    storage: SeaOrmStorage,
    cache: Arc<InMemoryPidCache>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RedeemRequest {
    pid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RedeemResponse {
    status: String,
    service_token: String,
    balance: i64,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug, Serialize)]
struct TokenStatusResponse {
    status: String,
    amount: i64,
    issued_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    abuse_score: i16,
}

#[derive(Debug, Deserialize, Serialize)]
struct RevokeRequest {
    reason: Option<String>,
    abuse_score: Option<i16>,
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("invalid payment id: {0}")]
    InvalidPid(#[from] PidFormatError),
    #[error("payment not found")]
    NotFound,
    #[error("payment already claimed")]
    AlreadyClaimed,
    #[error("token already revoked")]
    AlreadyRevoked,
    #[error("storage failure: {0}")]
    Storage(#[from] StorageError),
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::InvalidPid(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::AlreadyClaimed => StatusCode::CONFLICT,
            ApiError::AlreadyRevoked => StatusCode::CONFLICT,
            ApiError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: self.to_string(),
        })
    }
}

async fn redeem_handler(
    state: Data<AppState>,
    payload: web::Json<RedeemRequest>,
) -> Result<HttpResponse, ApiError> {
    let pid = PaymentId::parse(&payload.pid)?;

    match state.storage.claim_payment(&pid).await? {
        Some(outcome) => handle_success(&state, pid, outcome).await,
        None => handle_absent(&state, pid).await,
    }
}

async fn token_status_handler(
    state: Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let token = ServiceToken::new(path.into_inner());
    let record = state
        .storage
        .find_token(&token)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status = if record.revoked_at.is_some() {
        "revoked"
    } else {
        "active"
    };
    Ok(HttpResponse::Ok().json(TokenStatusResponse {
        status: status.to_string(),
        amount: record.amount,
        issued_at: record.issued_at,
        revoked_at: record.revoked_at,
        abuse_score: record.abuse_score,
    }))
}

async fn revoke_token_handler(
    state: Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<RevokeRequest>,
) -> Result<HttpResponse, ApiError> {
    let token = ServiceToken::new(path.into_inner());
    let existing = state
        .storage
        .find_token(&token)
        .await?
        .ok_or(ApiError::NotFound)?;
    if existing.revoked_at.is_some() {
        return Err(ApiError::AlreadyRevoked);
    }
    let updated = state
        .storage
        .revoke_token(RevokeTokenRequest {
            token,
            reason: payload.reason.clone(),
            abuse_score: payload.abuse_score,
        })
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(HttpResponse::Ok().json(TokenStatusResponse {
        status: "revoked".to_string(),
        amount: updated.amount,
        issued_at: updated.issued_at,
        revoked_at: updated.revoked_at,
        abuse_score: updated.abuse_score,
    }))
}

async fn handle_success(
    state: &AppState,
    pid: PaymentId,
    outcome: ClaimOutcome,
) -> Result<HttpResponse, ApiError> {
    let service_token = derive_service_token(&pid, &outcome.txid);
    state
        .storage
        .insert_token(NewServiceToken {
            token: service_token.clone(),
            pid: pid.clone(),
            amount: outcome.amount,
            issued_at: Utc::now(),
            abuse_score: 0,
        })
        .await?;
    state.cache.mark_present(&pid);

    Ok(HttpResponse::Ok().json(RedeemResponse {
        status: "success".to_string(),
        service_token: service_token.into_inner(),
        balance: outcome.amount,
    }))
}

async fn handle_absent(state: &AppState, pid: PaymentId) -> Result<HttpResponse, ApiError> {
    let maybe_payment = state.storage.find_payment(&pid).await?;
    match maybe_payment {
        Some(record) if record.status == PaymentStatus::Claimed => {
            state.cache.mark_present(&pid);
            Err(ApiError::AlreadyClaimed)
        }
        Some(_) => {
            state.cache.mark_present(&pid);
            Err(ApiError::NotFound)
        }
        None => {
            state.cache.mark_absent(&pid);
            Err(ApiError::NotFound)
        }
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    if let Err(err) = run().await {
        eprintln!("[api] bootstrap failed: {err}");
        return Err(io::Error::other(err.to_string()));
    }

    Ok(())
}

#[derive(Debug, Error)]
enum BootstrapError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

async fn run() -> Result<(), BootstrapError> {
    let config = ApiConfig::load_from_env()?;
    let storage = SeaOrmStorage::connect(config.database_url()).await?;
    let cache = Arc::new(InMemoryPidCache::default());
    let state = AppState { storage, cache };

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(state.clone()))
            .wrap(Logger::default())
            .route("/api/v1/redeem", web::post().to(redeem_handler))
            .route("/api/v1/token/{token}", web::get().to(token_status_handler))
            .route(
                "/api/v1/token/{token}/revoke",
                web::post().to(revoke_token_handler),
            )
    })
    .bind(config.api_bind_address())?
    .run()
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, test, App};
    use anon_ticket_domain::storage::{NewPayment, NewServiceToken};

    fn test_pid() -> PaymentId {
        PaymentId::new("0123456789abcdef0123456789abcdef")
    }

    async fn storage() -> SeaOrmStorage {
        SeaOrmStorage::connect("sqlite::memory:")
            .await
            .expect("storage inits")
    }

    fn with_cache(storage: SeaOrmStorage) -> AppState {
        AppState {
            storage,
            cache: Arc::new(InMemoryPidCache::default()),
        }
    }

    async fn insert_token(storage: &SeaOrmStorage) -> ServiceToken {
        let token =
            ServiceToken::new("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
        storage
            .insert_token(NewServiceToken {
                token: token.clone(),
                pid: test_pid(),
                amount: 42,
                issued_at: Utc::now(),
                abuse_score: 0,
            })
            .await
            .unwrap();
        token
    }

    #[actix_web::test]
    async fn rejects_invalid_pid_format() {
        let state = with_cache(storage().await);
        let app = test::init_service(
            App::new()
                .app_data(Data::new(state))
                .route("/api/v1/redeem", web::post().to(redeem_handler)),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/api/v1/redeem")
            .set_json(&RedeemRequest {
                pid: "short".into(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn returns_not_found_when_pid_missing() {
        let state = with_cache(storage().await);
        let app = test::init_service(
            App::new()
                .app_data(Data::new(state))
                .route("/api/v1/redeem", web::post().to(redeem_handler)),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/api/v1/redeem")
            .set_json(&RedeemRequest {
                pid: test_pid().into_inner(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn redeems_successfully() {
        let storage = storage().await;
        storage
            .insert_payment(NewPayment {
                pid: test_pid(),
                txid: "tx1".into(),
                amount: 42,
                block_height: 100,
                detected_at: Utc::now(),
            })
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(Data::new(with_cache(storage)))
                .route("/api/v1/redeem", web::post().to(redeem_handler)),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/api/v1/redeem")
            .set_json(&RedeemRequest {
                pid: test_pid().into_inner(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body()).await.unwrap();
        let parsed: RedeemResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed.balance, 42);
        assert_eq!(parsed.status, "success");
    }

    #[actix_web::test]
    async fn duplicate_claims_conflict() {
        let storage = storage().await;
        storage
            .insert_payment(NewPayment {
                pid: test_pid(),
                txid: "tx1".into(),
                amount: 42,
                block_height: 100,
                detected_at: Utc::now(),
            })
            .await
            .unwrap();
        storage.claim_payment(&test_pid()).await.unwrap();

        let app = test::init_service(
            App::new()
                .app_data(Data::new(with_cache(storage)))
                .route("/api/v1/redeem", web::post().to(redeem_handler)),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/api/v1/redeem")
            .set_json(&RedeemRequest {
                pid: test_pid().into_inner(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[actix_web::test]
    async fn token_status_returns_active() {
        let storage = storage().await;
        let token = insert_token(&storage).await;
        let app = test::init_service(
            App::new()
                .app_data(Data::new(with_cache(storage)))
                .route("/api/v1/token/{token}", web::get().to(token_status_handler)),
        )
        .await;
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/token/{}", token.as_str()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn revoke_token_marks_revoked() {
        let storage = storage().await;
        let token = insert_token(&storage).await;
        let app = test::init_service(
            App::new()
                .app_data(Data::new(with_cache(storage)))
                .route("/api/v1/token/{token}", web::get().to(token_status_handler))
                .route(
                    "/api/v1/token/{token}/revoke",
                    web::post().to(revoke_token_handler),
                ),
        )
        .await;

        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/token/{}/revoke", token.as_str()))
            .set_json(&RevokeRequest {
                reason: Some("abuse".into()),
                abuse_score: Some(5),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/token/{}", token.as_str()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
