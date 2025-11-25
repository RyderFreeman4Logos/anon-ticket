use std::{sync::Arc, time::Duration};

use actix_web::{body::to_bytes, test, web, App};
use anon_ticket_domain::model::{
    derive_service_token, NewPayment, NewServiceToken, PaymentId, ServiceToken,
};
use anon_ticket_domain::services::{
    cache::{InMemoryPidCache, PidBloom},
    telemetry::{init_telemetry, TelemetryConfig, TelemetryGuard},
};
use anon_ticket_domain::{PaymentStore, PidCache, TokenStore};
use anon_ticket_storage::SeaOrmStorage;
use chrono::Utc;
use tokio::time::sleep;

use crate::handlers::{
    redeem::{redeem_handler, RedeemRequest, RedeemResponse},
    token::{revoke_token_handler, token_status_handler, RevokeRequest, TokenStatusResponse},
};
use crate::state::AppState;

const DEFAULT_NEGATIVE_GRACE: Duration = Duration::from_millis(500);

fn test_pid() -> PaymentId {
    PaymentId::parse("0123456789abcdef").unwrap()
}

async fn storage() -> SeaOrmStorage {
    SeaOrmStorage::connect("sqlite::memory:")
        .await
        .expect("storage inits")
}

fn telemetry() -> TelemetryGuard {
    let config = TelemetryConfig::from_env("API_TEST");
    init_telemetry(&config).expect("telemetry inits")
}

fn build_state(
    storage: SeaOrmStorage,
    cache: Arc<InMemoryPidCache>,
    negative_grace: Duration,
) -> AppState {
    let telemetry = telemetry();
    let bloom = PidBloom::new(10_000, 0.01).ok().map(Arc::new);
    AppState::new(storage, cache, telemetry.clone(), negative_grace, bloom)
}

fn with_cache(storage: SeaOrmStorage) -> AppState {
    build_state(
        storage,
        Arc::new(InMemoryPidCache::default()),
        DEFAULT_NEGATIVE_GRACE,
    )
}

fn with_cache_ttl(storage: SeaOrmStorage, ttl: Duration) -> AppState {
    build_state(
        storage,
        Arc::new(InMemoryPidCache::new(ttl)),
        DEFAULT_NEGATIVE_GRACE,
    )
}

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

#[actix_web::test]
async fn rejects_invalid_pid_format() {
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
            pid: "short".into(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

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
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = to_bytes(resp.into_body()).await.unwrap();
    let parsed: RedeemResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.balance, 42);
    assert_eq!(parsed.status, "success");
}

#[actix_web::test]
async fn duplicate_claims_return_existing_token() {
    let storage = storage().await;
    let pid = test_pid();
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
    assert_eq!(parsed.status, "already_claimed");
    let expected = derive_service_token(&pid, "tx1");
    assert_eq!(parsed.service_token, expected.into_inner());
}

#[actix_web::test]
async fn cached_absence_short_circuits_requests() {
    let storage = storage().await;
    let pid = test_pid();
    let state = with_cache(storage.clone());
    state.cache().mark_absent(&pid);

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
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn cached_absence_grace_window_allows_redemption() {
    let storage = storage().await;
    let pid = test_pid();
    let state = with_cache_ttl(storage.clone(), Duration::from_secs(60));
    state.cache().mark_absent(&pid);

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

    sleep(DEFAULT_NEGATIVE_GRACE + Duration::from_millis(50)).await;

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

#[actix_web::test]
async fn cached_absence_expires_and_allows_redemption() {
    let storage = storage().await;
    let pid = test_pid();
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

    sleep(Duration::from_millis(30)).await;

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

#[actix_web::test]
async fn revoke_token_is_internal_only_and_revokes() {
    let storage = storage().await;
    let token = insert_token(&storage).await;
    let state = with_cache(storage);

    let public_app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/api/v1/token/{token}", web::get().to(token_status_handler)),
    )
    .await;

    let internal_app = test::init_service(App::new().app_data(web::Data::new(state)).route(
        "/api/v1/token/{token}/revoke",
        web::post().to(revoke_token_handler),
    ))
    .await;

    let revoke_body = RevokeRequest {
        reason: Some("abuse".into()),
        abuse_score: Some(5),
    };

    let public_resp = test::call_service(
        &public_app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/token/{}/revoke", token.to_hex()))
            .set_json(&revoke_body)
            .to_request(),
    )
    .await;
    assert_eq!(public_resp.status(), actix_web::http::StatusCode::NOT_FOUND);

    let internal_resp = test::call_service(
        &internal_app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/token/{}/revoke", token.to_hex()))
            .set_json(&revoke_body)
            .to_request(),
    )
    .await;
    assert_eq!(internal_resp.status(), actix_web::http::StatusCode::OK);

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
