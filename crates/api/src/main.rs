//! Actix-Web API exposing the redemption endpoint.

use std::io;

use actix_web::{
    http::StatusCode,
    middleware::Logger,
    web::{self, Data},
    App, HttpResponse, HttpServer, ResponseError,
};
use anon_ticket_domain::{
    derive_service_token,
    storage::{ClaimOutcome, NewServiceToken, PaymentStatus, PaymentStore, TokenStore},
    BootstrapConfig, ConfigError, PaymentId, PidFormatError, StorageError,
};
use anon_ticket_storage::SeaOrmStorage;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone)]
struct AppState {
    storage: SeaOrmStorage,
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

#[derive(Debug, Error)]
enum ApiError {
    #[error("invalid payment id: {0}")]
    InvalidPid(#[from] PidFormatError),
    #[error("payment not found")]
    NotFound,
    #[error("payment already claimed")]
    AlreadyClaimed,
    #[error("storage failure: {0}")]
    Storage(#[from] StorageError),
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::InvalidPid(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::AlreadyClaimed => StatusCode::CONFLICT,
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

    Ok(HttpResponse::Ok().json(RedeemResponse {
        status: "success".to_string(),
        service_token: service_token.into_inner(),
        balance: outcome.amount,
    }))
}

async fn handle_absent(state: &AppState, pid: PaymentId) -> Result<HttpResponse, ApiError> {
    let maybe_payment = state.storage.find_payment(&pid).await?;
    match maybe_payment {
        Some(record) if record.status == PaymentStatus::Claimed => Err(ApiError::AlreadyClaimed),
        Some(_) => Err(ApiError::NotFound),
        None => Err(ApiError::NotFound),
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
    let config = BootstrapConfig::load_from_env()?;
    let storage = SeaOrmStorage::connect(config.database_url()).await?;
    let state = AppState { storage };

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(state.clone()))
            .wrap(Logger::default())
            .route("/api/v1/redeem", web::post().to(redeem_handler))
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
    use anon_ticket_domain::storage::NewPayment;

    fn test_pid() -> PaymentId {
        PaymentId::new("0123456789abcdef0123456789abcdef")
    }

    async fn storage() -> SeaOrmStorage {
        SeaOrmStorage::connect("sqlite::memory:")
            .await
            .expect("storage inits")
    }

    #[actix_web::test]
    async fn rejects_invalid_pid_format() {
        let state = AppState {
            storage: storage().await,
        };
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
        let state = AppState {
            storage: storage().await,
        };
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
                .app_data(Data::new(AppState { storage }))
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
                .app_data(Data::new(AppState { storage }))
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
}
