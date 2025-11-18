pub mod metrics;
pub mod redeem;
pub mod token;

pub use metrics::metrics_handler;
pub use redeem::redeem_handler;
pub use token::{revoke_token_handler, token_status_handler};

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;
use thiserror::Error;

use anon_ticket_domain::model::PidFormatError;
use anon_ticket_domain::storage::StorageError;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid payment id: {0}")]
    InvalidPid(#[from] PidFormatError),
    #[error("payment not found")]
    NotFound,
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

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}
