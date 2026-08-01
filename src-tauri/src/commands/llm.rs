//! LLM commands —— 对标 routes::llm。
//!
//! 非流式：同步阻塞调 LlmClient::complete，返回 CompletionResponse。
//! 流式：调用方先指定 `request_id`，后端 spawn tokio 任务持续 emit
//!   `llm-stream` 事件（payload 含 request_id + StreamEvent），前端通过
//!   `listen("llm-stream", ...)` 接收，按 request_id 多路复用。

use std::sync::Arc;

use ats_core::failure::ActionableFailure;
use ats_core::llm::{
    CompletionRequest, CompletionResponse, LlmClient, StreamEvent, build_from_config,
};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::AppConfig;
use crate::commands::failure::{CommandFailure, CommandResult};

const STREAM_EVENT: &str = "llm-stream";

fn build_client(config: &AppConfig) -> CommandResult<Arc<dyn LlmClient>> {
    let settings = config.settings_snapshot();
    build_from_config(&settings.llm).map_err(|error| CommandFailure::llm("llm.configure", &error))
}

#[tauri::command]
pub async fn llm_complete(
    config: State<'_, AppConfig>,
    request: CompletionRequest,
) -> CommandResult<CompletionResponse> {
    let client = build_client(&config)?;
    client
        .complete(request)
        .await
        .map_err(|error| CommandFailure::llm("llm.complete", &error))
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
        failure: Box<ActionableFailure>,
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
) -> CommandResult<()> {
    let client = build_client(&config)?;
    let mut stream = client
        .stream(request)
        .await
        .map_err(|error| CommandFailure::llm("llm.stream_start", &error))?;

    let app_handle = app.clone();
    let req_id = request_id.clone();

    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            let payload = match item {
                Ok(event) => StreamPayload::Event {
                    request_id: req_id.clone(),
                    event,
                },
                Err(error) => StreamPayload::Error {
                    request_id: req_id.clone(),
                    failure: Box::new(ats_core::failure::FailureNormalizer::llm(
                        "llm.stream",
                        &error,
                    )),
                },
            };
            if let Err(e) = app_handle.emit(STREAM_EVENT, &payload) {
                eprintln!("llm stream emit failed: {e}");
                break;
            }
        }
        let _ = app_handle.emit(STREAM_EVENT, &StreamPayload::Done { request_id: req_id });
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_stream_failure_preserves_the_event_json_contract() {
        let payload = StreamPayload::Error {
            request_id: "request-1".into(),
            failure: Box::new(ActionableFailure::unclassified("llm.stream")),
        };

        let serialized = serde_json::to_value(payload).unwrap();
        assert_eq!(serialized["type"], "error");
        assert_eq!(serialized["request_id"], "request-1");
        assert_eq!(serialized["failure"]["code"], "core.unclassified");
        assert!(serialized["failure"].get("failure").is_none());
    }
}
