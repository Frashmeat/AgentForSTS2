//! LLM HTTP 路由——非流式 JSON + 流式 SSE。
//!
//! 非流式：POST /api/llm/complete → 200 JSON CompletionResponse
//! 流式：  POST /api/llm/stream   → 200 text/event-stream，每事件为 JSON 序列化的 StreamEvent

use std::sync::Arc;
use std::time::Duration;

use ats_core::llm::{
    AnthropicClient, CompletionRequest, CompletionResponse, LlmClient, LlmError, RetryConfig,
    RetryingClient, StreamEvent,
};
use axum::{
    Extension, Json, Router,
    http::StatusCode,
    response::{IntoResponse, Sse, sse::Event},
    routing::post,
};
use futures_util::{Stream, StreamExt};
use serde::Serialize;

use crate::AppState;

pub fn router() -> Router {
    Router::new()
        .route("/api/llm/complete", post(complete_handler))
        .route("/api/llm/stream", post(stream_handler))
}

fn build_client(state: &AppState) -> Result<Arc<dyn LlmClient>, (StatusCode, String)> {
    let llm = &state.config_status; // 这里只用 status，真实配置应从 settings 读
    let _ = llm; // 占位，真实拿 settings 需要把 Settings 注入 AppState（见 main.rs TODO）
    let settings = state
        .settings_snapshot
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "settings not loaded".to_string()))?;

    let api_key = settings.llm.api_key.clone();
    if api_key.is_empty() {
        return Err((
            StatusCode::PRECONDITION_FAILED,
            "llm.api_key not configured; set SPIREFORGE_LLM__API_KEY or fill config".into(),
        ));
    }
    let model = if settings.llm.model.is_empty() {
        "claude-opus-4-1-20250805".to_string()
    } else {
        settings.llm.model.clone()
    };
    let base_url = if settings.llm.base_url.is_empty() {
        None
    } else {
        Some(settings.llm.base_url.clone())
    };
    let client = AnthropicClient::new(api_key, model, base_url)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Arc::new(RetryingClient::new(
        Arc::new(client),
        RetryConfig::default(),
    )))
}

async fn complete_handler(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CompletionRequest>,
) -> Result<Json<CompletionResponse>, (StatusCode, String)> {
    let client = build_client(&state)?;
    client
        .complete(request)
        .await
        .map(Json)
        .map_err(llm_error_to_response)
}

async fn stream_handler(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CompletionRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, (StatusCode, String)>
{
    let client = build_client(&state)?;
    let llm_stream = client.stream(request).await.map_err(llm_error_to_response)?;

    let sse_stream = llm_stream.map(|item| {
        let payload = match item {
            Ok(ev) => SsePayload::Event(ev),
            Err(err) => SsePayload::Error(err.to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
        Ok(Event::default().data(json))
    });

    Ok(Sse::new(sse_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SsePayload {
    Event(StreamEvent),
    Error(String),
}

fn llm_error_to_response(err: LlmError) -> (StatusCode, String) {
    let status = match &err {
        LlmError::Auth(_) => StatusCode::UNAUTHORIZED,
        LlmError::RateLimit { .. } => StatusCode::TOO_MANY_REQUESTS,
        LlmError::Config(_) => StatusCode::PRECONDITION_FAILED,
        LlmError::Http { status: 400..=499, .. } => StatusCode::BAD_REQUEST,
        _ => StatusCode::BAD_GATEWAY,
    };
    (status, err.to_string())
}

#[allow(dead_code)]
fn _force_intoresponse_compile_check() -> axum::response::Response {
    (StatusCode::OK, "ok").into_response()
}
