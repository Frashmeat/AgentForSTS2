//! LLM HTTP 路由——非流式 JSON + 流式 SSE。
//!
//! 非流式：POST /api/llm/complete → 200 JSON CompletionResponse
//! 流式：  POST /api/llm/stream   → 200 text/event-stream，每事件为 JSON 序列化的 StreamEvent

use std::sync::Arc;
use std::time::Duration;

use ats_core::llm::{
    CompletionRequest, CompletionResponse, LlmClient, LlmError, StreamEvent, build_from_config,
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
    let settings = state
        .settings_snapshot
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "settings not loaded".to_string()))?;
    build_from_config(&settings.llm).map_err(|e| {
        let status = match &e {
            LlmError::Config(_) => StatusCode::PRECONDITION_FAILED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, e.to_string())
    })
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
            Ok(ev) => SsePayload::Event { event: ev },
            Err(err) => SsePayload::Error {
                message: err.to_string(),
            },
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

/// SSE envelope —— 与 src-tauri commands/llm.rs::StreamPayload 同形态
/// （`{type:"event",event:{...}}` / `{type:"error",message:"..."}`），
/// 前端 llmStream.ts 用同一 dispatch 处理双壳。
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SsePayload {
    Event { event: StreamEvent },
    Error { message: String },
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
