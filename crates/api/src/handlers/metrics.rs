// 引入 actix-web 组件：
// `web::Data`: 用于访问应用程序共享状态（State）。
// `HttpResponse`: 用于构建 HTTP 响应。
use actix_web::{web::Data, HttpResponse};

// 引入应用状态定义。
use crate::state::AppState;

// 定义指标处理函数 `metrics_handler`。
// 这是一个异步处理函数，接收应用状态 `state`。
// 主要用于 Prometheus 等监控系统抓取指标数据。
pub async fn metrics_handler(state: Data<AppState>) -> HttpResponse {
    // 调用遥测守卫的 `render_metrics` 方法，获取当前收集到的所有指标数据的文本表示。
    // 这通常是 Prometheus 的文本格式。
    let body = state.telemetry().render_metrics();
    
    // 构建并返回 HTTP 200 OK 响应。
    HttpResponse::Ok()
        // 设置 Content-Type 为纯文本，版本 0.0.4（Prometheus 标准格式）。
        .content_type("text/plain; version=0.0.4")
        // 将渲染好的指标文本作为响应体返回。
        .body(body)
}