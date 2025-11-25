// 引入标准库：
// `Arc`: 原子引用计数，用于共享状态。
// `Duration`: 时间段。
use std::{sync::Arc, time::Duration};

// 引入 actix-web 测试工具：
// `body::to_bytes`: 将响应体转换为字节。
// `test`: 测试辅助模块。
// `web`: Web 组件。
// `App`: App 构建器。
use actix_web::{body::to_bytes, test, web, App};

// 引入领域模型。
use anon_ticket_domain::model::{
    derive_service_token, NewPayment, NewServiceToken, PaymentId, ServiceToken,
};
// 引入服务：
// `InMemoryPidCache`: 内存缓存。
// `init_telemetry`: 初始化遥测。
use anon_ticket_domain::services::{
    cache::InMemoryPidCache,
    telemetry::{init_telemetry, TelemetryConfig, TelemetryGuard},
};
// 引入 trait 定义。
use anon_ticket_domain::{PaymentStore, PidCache, TokenStore};
// 引入存储实现。
use anon_ticket_storage::SeaOrmStorage;
// 引入时间库。
use chrono::Utc;
// 引入 tokio 的 sleep。
use tokio::time::sleep;

// 引入被测模块的处理函数和类型。
use crate::handlers::{
    redeem::{redeem_handler, RedeemRequest, RedeemResponse, PID_CACHE_NEGATIVE_GRACE},
    token::{revoke_token_handler, token_status_handler, RevokeRequest, TokenStatusResponse},
};
use crate::state::AppState;

// 辅助函数：生成一个固定的测试用 PaymentId。
fn test_pid() -> PaymentId {
    PaymentId::parse("0123456789abcdef").unwrap()
}

// 辅助函数：初始化一个内存数据库（SQLite内存模式）用于测试。
async fn storage() -> SeaOrmStorage {
    SeaOrmStorage::connect("sqlite::memory:")
        .await
        .expect("storage inits")
}

// 辅助函数：初始化测试用的遥测配置。
fn telemetry() -> TelemetryGuard {
    let config = TelemetryConfig::from_env("API_TEST");
    init_telemetry(&config).expect("telemetry inits")
}

// 辅助函数：构建 AppState。
fn build_state(storage: SeaOrmStorage, cache: Arc<InMemoryPidCache>) -> AppState {
    let telemetry = telemetry();
    AppState::new(storage, cache, telemetry.clone())
}

// 辅助函数：构建带有默认缓存的 AppState。
fn with_cache(storage: SeaOrmStorage) -> AppState {
    build_state(storage, Arc::new(InMemoryPidCache::default()))
}

// 辅助函数：构建带有特定 TTL 缓存的 AppState。
fn with_cache_ttl(storage: SeaOrmStorage, ttl: Duration) -> AppState {
    build_state(storage, Arc::new(InMemoryPidCache::new(ttl)))
}

// 辅助函数：向存储中插入一个测试用的令牌。
async fn insert_token(storage: &SeaOrmStorage) -> ServiceToken {
    let token =
        ServiceToken::parse("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
            .unwrap();
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

// 测试用例：验证如果 Payment ID 格式不正确，API 应返回 400 Bad Request。
#[actix_web::test]
async fn rejects_invalid_pid_format() {
    let state = with_cache(storage().await);
    // 初始化服务应用
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/v1/redeem", web::post().to(redeem_handler)),
    )
    .await;
    // 构造请求：PID "short" 长度不足/格式不对
    let req = test::TestRequest::post()
        .uri("/api/v1/redeem")
        .set_json(&RedeemRequest {
            pid: "short".into(),
        })
        .to_request();
    // 调用服务
    let resp = test::call_service(&app, req).await;
    // 断言状态码
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

// 测试用例：验证如果 PID 格式正确但数据库中不存在，API 应返回 404 Not Found。
#[actix_web::test]
async fn returns_not_found_when_pid_missing() {
    let state = with_cache(storage().await);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
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
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

// 测试用例：验证正常兑换流程。
#[actix_web::test]
async fn redeems_successfully() {
    let storage = storage().await;
    // 先在数据库中预置一条支付记录
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
            .app_data(web::Data::new(with_cache(storage)))
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
    
    // 断言成功
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    // 验证响应体内容
    let body = to_bytes(resp.into_body()).await.unwrap();
    let parsed: RedeemResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.balance, 42);
    assert_eq!(parsed.status, "success");
}

// 测试用例：验证重复兑换（幂等性）。
#[actix_web::test]
async fn duplicate_claims_return_existing_token() {
    let storage = storage().await;
    let pid = test_pid();
    // 1. 插入支付
    storage
        .insert_payment(NewPayment {
            pid: pid.clone(),
            txid: "tx1".into(),
            amount: 42,
            block_height: 100,
            detected_at: Utc::now(),
        })
        .await
        .unwrap();
    // 2. 模拟已经在代码外部被认领了一次
    storage.claim_payment(&pid).await.unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(with_cache(storage)))
            .route("/api/v1/redeem", web::post().to(redeem_handler)),
    )
    .await;
    let req = test::TestRequest::post()
        .uri("/api/v1/redeem")
        .set_json(&RedeemRequest {
            pid: pid.clone().into_inner(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;
    
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = to_bytes(resp.into_body()).await.unwrap();
    let parsed: RedeemResponse = serde_json::from_slice(&body).unwrap();
    // 断言状态为 "already_claimed"
    assert_eq!(parsed.status, "already_claimed");
    // 验证返回的 token 是否与预期一致
    let expected = derive_service_token(&pid, "tx1");
    assert_eq!(parsed.service_token, expected.into_inner());
}

// 测试用例：验证缓存的“不存在”标记（负面缓存）是否生效。
#[actix_web::test]
async fn cached_absence_short_circuits_requests() {
    let storage = storage().await;
    let pid = test_pid();
    let state = with_cache(storage.clone());
    // 手动在缓存中标记该 PID 不存在
    state.cache().mark_absent(&pid);

    // 随后在数据库中插入该记录（模拟数据刚刚到达）
    storage
        .insert_payment(NewPayment {
            pid: pid.clone(),
            txid: "tx-new".into(),
            amount: 7,
            block_height: 55,
            detected_at: Utc::now(),
        })
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/v1/redeem", web::post().to(redeem_handler)),
    )
    .await;
    let req = test::TestRequest::post()
        .uri("/api/v1/redeem")
        .set_json(&RedeemRequest {
            pid: pid.clone().into_inner(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;
    // 因为缓存说不存在，且可能超过了宽限期（或逻辑判定短路），应该返回 404，尽管数据库里有了。
    // 注意：默认缓存配置下，如果不在 grace window 内，就会短路。
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

// 测试用例：验证在“负面宽限期”内，即使缓存标记为不存在，也允许穿透去查库。
#[actix_web::test]
async fn cached_absence_grace_window_allows_redemption() {
    let storage = storage().await;
    let pid = test_pid();
    // 设置较长的 TTL
    let state = with_cache_ttl(storage.clone(), Duration::from_secs(60));
    state.cache().mark_absent(&pid); // 刚刚标记为不存在

    // 立即插入数据
    storage
        .insert_payment(NewPayment {
            pid: pid.clone(),
            txid: "tx-grace".into(),
            amount: 9,
            block_height: 56,
            detected_at: Utc::now(),
        })
        .await
        .unwrap();

    // 等待一小段时间，但需保证 `now - absent_time < PID_CACHE_NEGATIVE_GRACE` 失败？
    // 哎呀，这里代码逻辑是：
    // sleep(PID_CACHE_NEGATIVE_GRACE + 50ms) -> 意味着超过了宽限期。
    // 如果超过了宽限期，且 TTL 还没过期，那么应该被阻挡（Not Found）。
    // 等等，看测试名字是 "allows_redemption"。
    // 让我再读一下 `redeem_handler` 逻辑：
    // age < GRACE -> should_short_circuit = true? 
    // 不，`is_some_and(|age| age < PID_CACHE_NEGATIVE_GRACE)`
    // 如果 age < grace，should_short_circuit 是 true。
    // if should_short_circuit { return NotFound }
    // 所以：如果在宽限期内（刚刚标记不存在），我们会直接返回 NotFound（短路）。
    // 
    // 这里的测试逻辑似乎有点反直觉：
    // 代码中：`if age < GRACE { short_circuit }` 
    // 意味着：如果你刚刚查过说没有，那么在这个极短的宽限期内，我都不会再去查库了（防止瞬间高并发击穿）。
    // 
    // 让我们看这个测试用例做了什么：
    // 1. mark_absent
    // 2. insert payment
    // 3. sleep(GRACE + 50ms) -> 此时 age > GRACE。
    // 4. 请求。
    // 此时 `age < GRACE` 为 false。`should_short_circuit` 为 false。
    // 所以它会去查库。
    // 结果应该是 OK。
    
    sleep(PID_CACHE_NEGATIVE_GRACE + Duration::from_millis(50)).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/v1/redeem", web::post().to(redeem_handler)),
    )
    .await;
    let req = test::TestRequest::post()
        .uri("/api/v1/redeem")
        .set_json(&RedeemRequest {
            pid: pid.into_inner(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = to_bytes(resp.into_body()).await.unwrap();
    let parsed: RedeemResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.status, "success");
}

// 测试用例：验证缓存过期后，负面标记失效，允许查库。
#[actix_web::test]
async fn cached_absence_expires_and_allows_redemption() {
    let storage = storage().await;
    let pid = test_pid();
    // 设置极短的 TTL (20ms)
    let state = with_cache_ttl(storage.clone(), Duration::from_millis(20));
    state.cache().mark_absent(&pid);

    storage
        .insert_payment(NewPayment {
            pid: pid.clone(),
            txid: "tx-expire".into(),
            amount: 11,
            block_height: 56,
            detected_at: Utc::now(),
        })
        .await
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/api/v1/redeem", web::post().to(redeem_handler)),
    )
    .await;

    // 立即请求（在 TTL 内，且在 GRACE 内），应该被阻挡。
    let blocked = test::TestRequest::post()
        .uri("/api/v1/redeem")
        .set_json(&RedeemRequest {
            pid: pid.clone().into_inner(),
        })
        .to_request();
    let blocked_resp = test::call_service(&app, blocked).await;
    assert_eq!(
        blocked_resp.status(),
        actix_web::http::StatusCode::NOT_FOUND
    );

    // 等待 TTL 过期 (30ms > 20ms)
    sleep(Duration::from_millis(30)).await;

    // 再次请求，缓存已过期，应穿透查库。
    let allowed = test::TestRequest::post()
        .uri("/api/v1/redeem")
        .set_json(&RedeemRequest {
            pid: pid.into_inner(),
        })
        .to_request();
    let allowed_resp = test::call_service(&app, allowed).await;
    assert_eq!(allowed_resp.status(), actix_web::http::StatusCode::OK);
    let body = to_bytes(allowed_resp.into_body()).await.unwrap();
    let parsed: RedeemResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.status, "success");
}

// 测试用例：验证查询 Token 状态。
#[actix_web::test]
async fn token_status_returns_active() {
    let storage = storage().await;
    let token = insert_token(&storage).await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(with_cache(storage)))
            .route("/api/v1/token/{token}", web::get().to(token_status_handler)),
    )
    .await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/token/{}", token.to_hex()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

// 测试用例：验证撤销 Token 功能（区分内部和外部接口）。
#[actix_web::test]
async fn revoke_token_is_internal_only_and_revokes() {
    let storage = storage().await;
    let token = insert_token(&storage).await;
    let state = with_cache(storage);

    // 模拟公共 App：只暴露查询接口
    let public_app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/api/v1/token/{token}", web::get().to(token_status_handler)),
    )
    .await;

    // 模拟内部 App：暴露撤销接口
    let internal_app = test::init_service(App::new().app_data(web::Data::new(state)).route(
        "/api/v1/token/{token}/revoke",
        web::post().to(revoke_token_handler),
    ))
    .await;

    let revoke_body = RevokeRequest {
        reason: Some("abuse".into()),
        abuse_score: Some(5),
    };

    // 尝试在公共 App 上撤销 -> 404 Not Found (路由不存在)
    let public_resp = test::call_service(
        &public_app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/token/{}/revoke", token.to_hex()))
            .set_json(&revoke_body)
            .to_request(),
    )
    .await;
    assert_eq!(public_resp.status(), actix_web::http::StatusCode::NOT_FOUND);

    // 在内部 App 上撤销 -> 200 OK
    let internal_resp = test::call_service(
        &internal_app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/token/{}/revoke", token.to_hex()))
            .set_json(&revoke_body)
            .to_request(),
    )
    .await;
    assert_eq!(internal_resp.status(), actix_web::http::StatusCode::OK);

    // 验证状态变为 revoked
    let status_resp = test::call_service(
        &public_app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/token/{}", token.to_hex()))
            .to_request(),
    )
    .await;
    assert_eq!(status_resp.status(), actix_web::http::StatusCode::OK);
    let parsed: TokenStatusResponse =
        serde_json::from_slice(&to_bytes(status_resp.into_body()).await.unwrap()).unwrap();
    assert_eq!(parsed.status, "revoked");
}
