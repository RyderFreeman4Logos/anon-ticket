// 引入 actix-web 核心组件。
use actix_web::{web, HttpResponse};
// 引入领域模型：
// `RevokeTokenRequest`: 撤销令牌请求模型。
// `ServiceToken`: 服务令牌类型。
use anon_ticket_domain::model::{RevokeTokenRequest, ServiceToken};
// 引入 TokenStore trait，用于操作令牌数据。
use anon_ticket_domain::storage::TokenStore;
// 引入时间处理库。
use chrono::{DateTime, Utc};
// 引入 metrics 库。
use metrics::counter;
// 引入 serde 用于序列化。
use serde::{Deserialize, Serialize};

// 引入应用状态。
use crate::state::AppState;

// 引入统一错误类型。
use super::ApiError;

// 定义令牌状态响应结构体。
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenStatusResponse {
    // 状态字符串： "active" 或 "revoked"。
    pub status: String,
    // 令牌对应的金额。
    pub amount: i64,
    // 发行时间。
    pub issued_at: DateTime<Utc>,
    // 撤销时间（如果已撤销）。
    pub revoked_at: Option<DateTime<Utc>>,
    // 滥用分数。
    pub abuse_score: i16,
}

// 定义撤销请求结构体。
#[derive(Debug, Deserialize, Serialize)]
pub struct RevokeRequest {
    // 撤销原因（可选）。
    pub reason: Option<String>,
    // 设置新的滥用分数（可选）。
    pub abuse_score: Option<i16>,
}

// 处理函数：查询令牌状态。
// GET /api/v1/token/{token}
pub async fn token_status_handler(
    state: web::Data<AppState>,
    path: web::Path<String>, // 从 URL路径参数中提取 token 字符串
) -> Result<HttpResponse, ApiError> {
    // 解析路径参数中的 token 字符串为 ServiceToken 类型。
    // 如果格式错误，会返回错误。
    let token = ServiceToken::parse(&path.into_inner())?;
    
    // 在存储层查找令牌记录。
    let record = match state.storage().find_token(&token).await? {
        Some(record) => record,
        None => {
            // 如果找不到，记录指标并返回 404。
            counter!("api_token_requests_total", 1, "endpoint" => "status", "status" => "not_found");
            return Err(ApiError::NotFound);
        }
    };

    // 根据 `revoked_at` 字段判断当前状态。
    let status = if record.revoked_at.is_some() {
        "revoked"
    } else {
        "active"
    };

    // 记录请求指标。
    counter!("api_token_requests_total", 1, "endpoint" => "status", "status" => status);

    // 返回包含详细信息的 JSON 响应。
    Ok(HttpResponse::Ok().json(TokenStatusResponse {
        status: status.to_string(),
        amount: record.amount,
        issued_at: record.issued_at,
        revoked_at: record.revoked_at,
        abuse_score: record.abuse_score,
    }))
}

// 处理函数：撤销令牌。
// POST /api/v1/token/{token}/revoke
pub async fn revoke_token_handler(
    state: web::Data<AppState>,
    path: web::Path<String>, // 路径参数：token
    payload: web::Json<RevokeRequest>, // 请求体：撤销详情
) -> Result<HttpResponse, ApiError> {
    // 解析 token。
    let token = ServiceToken::parse(&path.into_inner())?;
    
    // 首先检查令牌是否存在。
    let existing = match state.storage().find_token(&token).await? {
        Some(record) => record,
        None => {
            counter!("api_token_requests_total", 1, "endpoint" => "revoke", "status" => "not_found");
            return Err(ApiError::NotFound);
        }
    };

    // 检查是否已经是撤销状态。
    if existing.revoked_at.is_some() {
        counter!("api_token_requests_total", 1, "endpoint" => "revoke", "status" => "already_revoked");
        // 返回 409 Conflict 错误。
        return Err(ApiError::AlreadyRevoked);
    }

    // 执行撤销操作。
    // `revoke_token` 会更新数据库中的记录，设置 revoked_at 时间。
    let updated = state
        .storage()
        .revoke_token(RevokeTokenRequest {
            token,
            reason: payload.reason.clone(),
            abuse_score: payload.abuse_score,
        })
        .await?
        .ok_or(ApiError::NotFound)?; // 如果在更新间隙记录消失了（极不可能），按 NotFound 处理。

    // 记录成功指标。
    counter!("api_token_requests_total", 1, "endpoint" => "revoke", "status" => "revoked");

    // 返回更新后的状态信息。
    Ok(HttpResponse::Ok().json(TokenStatusResponse {
        status: "revoked".to_string(),
        amount: updated.amount,
        issued_at: updated.issued_at,
        revoked_at: updated.revoked_at,
        abuse_score: updated.abuse_score,
    }))
}