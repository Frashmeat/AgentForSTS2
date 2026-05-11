//! LLM commands —— 对标 routes::llm。
//!
//! 非流式：同步阻塞调 LlmClient::complete，返回 CompletionResponse。
//! 流式：调用方先指定 `request_id`，后端 spawn tokio 任务持续 emit
//!   `llm-stream` 事件（payload 含 request_id + StreamEvent），前端通过
//!   `listen("llm-stream", ...)` 接收，按 request_id 多路复用。

use std::sync::Arc;

use ats_core::llm::{
    AnthropicClient, CompletionRequest, CompletionResponse, LlmClient, RetryConfig, RetryingClient,
    StreamEvent,
};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::AppConfig;

const STREAM_EVENT: &str = "llm-stream";

fn build_client(config: &AppConfig) -> Result<Arc<dyn LlmClient>, String> {
    let settings = &config.settings;
    let api_key = settings.llm.api_key.clone();
    if api_key.is_empty() {
        return Err(
            "llm.api_key not configured; fill agentthespire.config.json or set SPIREFORGE_LLM__API_KEY"
                .into(),
        );
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
    let client = AnthropicClient::new(api_key, model, base_url).map_err(|e| e.to_string())?;
    Ok(Arc::new(RetryingClient::new(
        Arc::new(client),
        RetryConfig::default(),
    )))
}

#[tauri::command]
pub async fn llm_complete(
    config: State<'_, AppConfig>,
    request: CompletionRequest,
) -> Result<CompletionResponse, String> {
    let client = build_client(&config)?;
    client.complete(request).await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamPayload {
    Event {
        request_id: String,
        event: StreamEvent,
    },
    Error {
        request_id: String,
        message: String,
    },
    Done {
        request_id: String,
    },
}

#[tauri::command]
pub async fn llm_start_stream(
    app: AppHandle,
    config: State<'_, AppConfig>,
    request_id: String,
    request: CompletionRequest,
) -> Result<(), String> {
    let client = build_client(&config)?;
    let mut stream = client.stream(request).await.map_err(|e| e.to_string())?;

    let app_handle = app.clone();
    let req_id = request_id.clone();

    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            let payload = match item {
                Ok(event) => StreamPayload::Event {
                    request_id: req_id.clone(),
                    event,
                },
                Err(err) => StreamPayload::Error {
                    request_id: req_id.clone(),
                    message: err.to_string(),
                },
            };
            if let Err(e) = app_handle.emit(STREAM_EVENT, &payload) {
                eprintln!("llm stream emit failed: {e}");
                break;
            }
        }
        let _ = app_handle.emit(
            STREAM_EVENT,
            &StreamPayload::Done {
                request_id: req_id,
            },
        );
    });

    Ok(())
}
