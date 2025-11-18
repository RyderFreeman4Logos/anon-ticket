use actix_web::{web::Data, HttpResponse};

use crate::state::AppState;

pub async fn metrics_handler(state: Data<AppState>) -> HttpResponse {
    let body = state.telemetry().render_metrics();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(body)
}
