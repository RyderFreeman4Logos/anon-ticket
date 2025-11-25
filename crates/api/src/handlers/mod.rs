// 声明子模块：
// `metrics`: 处理指标相关的请求。
// `redeem`: 处理兑换（Redeem）相关的请求，即将支付转换为服务令牌。
// `token`: 处理令牌（Token）相关的请求，如查询状态和撤销。
pub mod metrics;
pub mod redeem;
pub mod token;

// 重新导出各个处理函数，方便外部（如 application.rs）直接引用。
pub use metrics::metrics_handler;
pub use redeem::redeem_handler;
pub use token::{revoke_token_handler, token_status_handler};

// 引入 actix-web 框架的核心组件：
// `StatusCode`: HTTP 状态码。
// `HttpResponse`: 构建 HTTP 响应。
// `ResponseError`: trait，用于将自定义错误转换为 HTTP 响应。
use actix_web::{http::StatusCode, HttpResponse, ResponseError};
// 引入 serde 的 `Serialize` trait，用于将结构体序列化为 JSON。
use serde::Serialize;
// 引入 `thiserror` 的 `Error` 宏，用于简化自定义错误的定义。
use thiserror::Error;

// 引入领域模型中的错误类型。
use anon_ticket_domain::model::{PidFormatError, TokenFormatError};
// 引入存储层的错误类型。
use anon_ticket_domain::storage::StorageError;

// 定义 API 统一错误枚举 `ApiError`。
// 这个枚举囊括了 API 层可能抛出的所有错误，并实现了 `thiserror::Error` 以提供错误描述。
#[derive(Debug, Error)]
pub enum ApiError {
    // 支付 ID 格式无效。
    #[error("invalid payment id: {0}")]
    InvalidPid(#[from] PidFormatError),
    // 令牌格式无效。
    #[error("invalid token: {0}")]
    InvalidToken(#[from] TokenFormatError),
    // 找不到支付记录或令牌。
    #[error("payment not found")]
    NotFound,
    // 令牌已经被撤销。
    #[error("token already revoked")]
    AlreadyRevoked,
    // 存储层（数据库）发生错误。
    #[error("storage failure: {0}")]
    Storage(#[from] StorageError),
}

// 为 `ApiError` 实现 `actix_web::ResponseError` trait。
// 这允许我们直接在处理函数中返回 `Result<HttpResponse, ApiError>`，
// actix-web 会自动调用此实现将错误转换为 HTTP 响应。
impl ResponseError for ApiError {
    // 定义每种错误对应的 HTTP 状态码。
    fn status_code(&self) -> StatusCode {
        match self {
            // 客户端错误：参数格式不对 -> 400 Bad Request
            ApiError::InvalidPid(_) => StatusCode::BAD_REQUEST,
            ApiError::InvalidToken(_) => StatusCode::BAD_REQUEST,
            // 资源不存在 -> 404 Not Found
            ApiError::NotFound => StatusCode::NOT_FOUND,
            // 资源冲突（已撤销） -> 409 Conflict
            ApiError::AlreadyRevoked => StatusCode::CONFLICT,
            // 服务器内部错误（数据库故障） -> 500 Internal Server Error
            ApiError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    // 定义错误的响应体内容。
    // 这里我们返回一个 JSON 对象 `ErrorBody`，包含具体的错误信息。
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: self.to_string(), // 调用 Display trait 获取错误描述
        })
    }
}

// 定义统一的错误响应结构体。
// 所有的 API 错误响应都将符合这个 JSON 结构：`{ "error": "..." }`。
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}
