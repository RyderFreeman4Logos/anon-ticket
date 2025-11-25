// 引入标准库的 Duration，用于处理时间间隔。
use std::time::Duration;

// 引入 actix-web 的核心组件。
use actix_web::{web, HttpResponse};
// 引入领域模型中的各种类型：
// `derive_service_token`: 用于从支付ID和交易哈希生成令牌的工具函数。
// `ClaimOutcome`: 支付声明的结果（如金额、时间）。
// `NewServiceToken`: 创建新令牌的结构体。
// `PaymentId`, `PaymentRecord`, `PaymentStatus`: 支付相关模型。
// `ServiceTokenRecord`: 服务令牌的数据库记录模型。
use anon_ticket_domain::model::{
    derive_service_token, ClaimOutcome, NewServiceToken, PaymentId, PaymentRecord, PaymentStatus,
    ServiceTokenRecord,
};
// 引入存储层接口 trait。
use anon_ticket_domain::storage::{PaymentStore, TokenStore};
// 引入缓存接口 trait。
use anon_ticket_domain::PidCache;
// 引入时间处理库 chrono。
use chrono::Utc;
// 引入 metrics 库，用于记录业务指标。
use metrics::counter;
// 引入 serde，用于 JSON 序列化和反序列化。
use serde::{Deserialize, Serialize};

// 引入应用状态。
use crate::state::AppState;

// 引入上层模块定义的 API 错误。
use super::ApiError;

// 定义 PID 缓存的“负面宽限期”。
// 如果一个请求在最近 500ms 内被标记为“不存在”（负面缓存），
// 但此时距离标记时间小于这个宽限期，我们仍然允许它穿透缓存去查数据库。
// 这是为了解决并发或极其短暂的同步延迟问题。
pub const PID_CACHE_NEGATIVE_GRACE: Duration = Duration::from_millis(500);

// 定义兑换请求的 JSON 结构体。
#[derive(Debug, Deserialize, Serialize)]
pub struct RedeemRequest {
    // 客户端提交的支付 ID 字符串。
    pub pid: String,
}

// 定义兑换响应的 JSON 结构体。
#[derive(Debug, Serialize, Deserialize)]
pub struct RedeemResponse {
    // 状态描述，如 "success", "already_claimed"。
    pub status: String,
    // 生成的服务令牌字符串。
    pub service_token: String,
    // 令牌关联的余额/金额。
    pub balance: i64,
}

// 核心处理函数：`redeem_handler`
// 负责处理 `/api/v1/redeem` POST 请求。
// 参数：
// - `state`: 应用共享状态。
// - `payload`: 解析后的 JSON 请求体。
pub async fn redeem_handler(
    state: web::Data<AppState>,
    payload: web::Json<RedeemRequest>,
) -> Result<HttpResponse, ApiError> {
    // 1. 解析 Payment ID
    // 尝试将字符串解析为强类型的 `PaymentId`。
    // 如果解析失败，记录指标并返回错误。
    let pid = PaymentId::parse(&payload.pid).inspect_err(|_| {
        counter!("api_redeem_requests_total", 1, "status" => "invalid_pid");
    })?;

    // 2. 检查缓存
    // 如果缓存中不包含该 PID（意味着可能之前被标记为不存在，或者从未见过）：
    if !state.cache().might_contain(&pid) {
        // 检查这是一个“硬”不存在，还是在宽限期内。
        // `negative_entry_age` 返回该条目被标记为不存在到现在经过的时间。
        let should_short_circuit = state
            .cache()
            .negative_entry_age(&pid)
            // 如果存在负面记录，且该记录非常新（小于宽限期），则允许通过（不短路）。
            // 否则（记录较旧），则短路拦截。
            .is_some_and(|age| age < PID_CACHE_NEGATIVE_GRACE);

        if should_short_circuit {
            // 记录被缓存拦截的指标。
            counter!("api_redeem_cache_hints_total", 1, "hint" => "absent_blocked");
            counter!("api_redeem_requests_total", 1, "status" => "cache_absent");
            // 直接返回 404 Not Found，避免查库。
            return Err(ApiError::NotFound);
        }

        // 记录虽然不在缓存中，但被允许探测数据库的指标（宽限期机制生效）。
        counter!("api_redeem_cache_hints_total", 1, "hint" => "absent_probe");
    }

    // 3. 尝试在存储层“认领”该支付
    // `claim_payment` 是一个原子操作：如果支付存在且未被认领，则将其标记为已认领并返回 Outcome。
    match state.storage().claim_payment(&pid).await? {
        // 场景 A: 认领成功（首次兑换）。
        Some(outcome) => handle_success(&state, pid, outcome).await,
        // 场景 B: 认领失败（支付不存在，或已被认领）。
        None => handle_absent(&state, pid).await,
    }
}

// 辅助函数：处理认领成功的情况。
async fn handle_success(
    state: &AppState,
    pid: PaymentId,
    outcome: ClaimOutcome,
) -> Result<HttpResponse, ApiError> {
    // 根据 PID 和交易 ID 确定性地派生服务令牌。
    let service_token = derive_service_token(&pid, &outcome.txid);
    
    // 将新生成的令牌插入数据库。
    let token_record = state
        .storage()
        .insert_token(NewServiceToken {
            token: service_token,
            pid: pid.clone(),
            amount: outcome.amount,
            issued_at: outcome.claimed_at,
            abuse_score: 0, // 初始滥用分数为 0
        })
        .await?;
    
    // 记录成功指标。
    counter!("api_redeem_requests_total", 1, "status" => "success");
    // 更新缓存：标记该 PID 为“存在”，以便后续请求能快速命中缓存（虽然已被认领，但存在）。
    state.cache().mark_present(&pid);

    // 返回成功响应。
    Ok(HttpResponse::Ok().json(build_redeem_response("success", token_record)))
}

// 辅助函数：处理 `claim_payment` 返回 None 的情况。
// 这意味着支付要么不存在，要么已经被认领了。我们需要进一步查询以区分这两种情况。
async fn handle_absent(state: &AppState, pid: PaymentId) -> Result<HttpResponse, ApiError> {
    // 查询支付记录详情。
    let maybe_payment = state.storage().find_payment(&pid).await?;
    match maybe_payment {
        // 情况 1: 支付记录存在，且状态为 `Claimed`。
        // 这意味着用户重复提交了兑换请求。
        Some(record) if record.status == PaymentStatus::Claimed => {
            // 确保缓存标记为存在。
            state.cache().mark_present(&pid);
            // 获取或恢复对应的令牌记录。
            let token = ensure_token_record(state, &pid, &record).await?;
            // 记录重复认领指标。
            counter!("api_redeem_requests_total", 1, "status" => "already_claimed");
            // 返回成功响应，但状态为 "already_claimed"，并返回之前的令牌。
            // 这是幂等性的体现：重复请求返回相同结果。
            Ok(HttpResponse::Ok().json(build_redeem_response("already_claimed", token)))
        }
        // 情况 2: 支付记录存在，但状态不是 Claimed（例如 Pending）。
        // 理论上 `claim_payment` 应该能处理 Pending 状态，这里作为防御性编程。
        Some(_) => {
            state.cache().mark_present(&pid);
            counter!("api_redeem_requests_total", 1, "status" => "pending");
            // 暂时返回 Not Found，或者可以返回 202 Accepted 表示处理中。
            Err(ApiError::NotFound)
        }
        // 情况 3: 支付记录根本不存在。
        None => {
            // 更新缓存：标记该 PID 为“不存在”（负面缓存），防止缓存穿透。
            state.cache().mark_absent(&pid);
            counter!("api_redeem_requests_total", 1, "status" => "not_found");
            Err(ApiError::NotFound)
        }
    }
}

// 辅助函数：构建响应对象。
fn build_redeem_response(status: &str, record: ServiceTokenRecord) -> RedeemResponse {
    RedeemResponse {
        status: status.to_string(),
        service_token: record.token.into_inner(),
        balance: record.amount,
    }
}

// 辅助函数：确保能够获取到令牌记录。
// 在重复认领的情况下，我们需要返回已存在的令牌。
async fn ensure_token_record(
    state: &AppState,
    pid: &PaymentId,
    payment: &PaymentRecord,
) -> Result<ServiceTokenRecord, ApiError> {
    // 重新派生令牌。
    let token = derive_service_token(pid, &payment.txid);
    
    // 1. 尝试直接查询令牌。
    if let Some(existing) = state.storage().find_token(&token).await? {
        return Ok(existing);
    }
    
    // 2. 如果没找到（极罕见情况，如数据不一致），尝试重新插入。
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
        // 如果插入时发生唯一性冲突（"unique"），说明并发情况下令牌已存在。
        // 此时再次查询即可。
        Err(ApiError::Storage(err)) if err.to_string().to_lowercase().contains("unique") => state
            .storage()
            .find_token(&token)
            .await?
            .ok_or(ApiError::NotFound),
        // 其他错误直接返回。
        Err(other) => Err(other),
    }
}
