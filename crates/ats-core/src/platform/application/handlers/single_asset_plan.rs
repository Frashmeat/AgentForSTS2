//! single_asset_plan handler：自然语言需求 → LLM 输出 JSON → 解析为 PlanItem。
//!
//! Prompt 中固定声明 JSON schema（与 PlanItem 字段对齐），让 LLM 出严格 JSON。
//! 解析阶段宽容：
//! - LLM 可能用 ```json fence 包裹，复用 code_generate 的 extract_first_code_block
//! - 字段缺失走 PlanItem::default() 兜底（PlanItem 自带 #[serde(default)])

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;

use super::code_generate::extract_first_code_block;
use super::common::{
    ProgressEvent, ProgressSink, emit_cancelled_mid_stream, finalize_with_error, is_cancelled,
    transition_to_running,
};
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::planning::PlanItem;
use crate::platform::contracts::SubmitSingleAssetPlanRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};

const SYSTEM_PROMPT: &str = "你是 Slay the Spire 2 mod 开发的策划助手。\n\
用户会给出一个自然语言需求和（可选的）资产类型，请输出**单个** PlanItem 的严格 JSON。\n\n\
JSON 字段（snake_case，全部必填，未涉及的字段输出空字符串或空数组）：\n\
- id: 短英文标识，蛇形命名，例如 \"flame_relic_v1\"\n\
- type: 资产类型，枚举值之一：\"card\" / \"card_fullscreen\" / \"relic\" / \"power\" / \"character\" / \"custom_code\"\n\
- name: 英文名（标题大小写）\n\
- name_zhs: 简体中文名\n\
- description: 一句话英文描述（卡面/遗物效果原文风格）\n\
- goal: 设计目标，一句话\n\
- detailed_description: 详细中文设计说明，3-6 句\n\
- implementation_notes: 行为意图、数值约束和需要核实的源码证据，中文；没有当前源码证据时不得猜测具体 hook 名\n\
- needs_image: bool，是否需要美术图\n\
- image_description: 若 needs_image=true，描述图片内容；否则空字符串\n\
- depends_on_item_ids: 字符串数组，依赖项 id 列表\n\
- scope_boundary: 此 item 不做哪些事，中文\n\
- relationship_reason: 与依赖项的联系，中文\n\
- acceptance_notes: 完成判定条件，中文\n\
- affected_targets: 影响目标字符串数组（如 [\"player\", \"enemies\"]）\n\
- relationship_type: \"independent\" / \"depends_on\" / \"unknown\"\n\
- clarification_status: 一般填 \"\"，若有未决问题填 \"pending\"\n\
- clarification_questions: 若有不明确点列出，否则空数组\n\
- provided_image_b64: 一律输出 \"\"\n\n\
**重要**：规划阶段不提供游戏源码。不要把方法名看作时序证据，也不要发明 OnCombatStart、OnPlayerStartTurn 等 hook。需要 hook 的行为应在 implementation_notes 中写明“生成时从当前源码检索官方相似实现和生命周期调用方”。\n\
只输出 JSON 对象本体，不要附加说明、不要 markdown fence。\n";

pub async fn run_single_asset_plan(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitSingleAssetPlanRequest,
    items_dir: Option<PathBuf>,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }
    if request.requirements.trim().is_empty() {
        finalize_with_error(&repo, &job_id, &sink, "requirements is empty").await;
        return;
    }

    let user_prompt = build_user_prompt(&request);
    let completion_request = CompletionRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: user_prompt,
        }],
        system_prompt: Some(SYSTEM_PROMPT.to_string()),
        max_tokens: request.max_tokens.unwrap_or(2048),
        temperature: Some(0.4),
        model: None,
    };

    let mut stream = match llm.stream(completion_request).await {
        Ok(s) => s,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &sink, &err.to_string()).await;
            return;
        }
    };

    let mut accumulated = String::new();
    let mut model = String::new();
    let mut usage_in: u32 = 0;
    let mut usage_out: u32 = 0;
    let mut tick: u32 = 0;
    while let Some(item) = stream.next().await {
        tick = tick.wrapping_add(1);
        if tick.is_multiple_of(5) && is_cancelled(&repo, &job_id).await {
            emit_cancelled_mid_stream(&sink, &job_id).await;
            return;
        }
        match item {
            Ok(StreamEvent::Start { model: m }) => {
                model = m;
            }
            Ok(StreamEvent::Delta { text }) => {
                accumulated.push_str(&text);
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
                    stage: "stream-delta".into(),
                    percent: None,
                    message: None,
                    delta: Some(text),
                })
                .await;
            }
            Ok(StreamEvent::End { usage, .. }) => {
                usage_in = usage.input_tokens;
                usage_out = usage.output_tokens;
            }
            Err(err) => {
                finalize_with_error(&repo, &job_id, &sink, &err.to_string()).await;
                return;
            }
        }
    }

    let job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(_) => return,
    };
    if matches!(job.status, JobStatus::Cancelled) {
        return;
    }

    let plan_item = match parse_plan_item(&accumulated) {
        Ok(p) => p,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &sink, &format!(
                "parse plan json: {err}; raw: {}",
                truncate(&accumulated, 500)
            ))
            .await;
            return;
        }
    };

    // 把 PlanItem 落到 <items_dir>/<id>.json，让用户在工程目录里能看到 plan
    // 产物。写失败不致命——job.result 仍可保留 item。
    let item_file_path = if let Some(dir) = &items_dir {
        match persist_plan_item(dir, &plan_item).await {
            Ok(p) => Some(p),
            Err(err) => {
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
                    stage: "items-write-warn".into(),
                    percent: None,
                    message: Some(format!("write items file failed: {err}")),
                    delta: None,
                })
                .await;
                None
            }
        }
    } else {
        None
    };

    let mut job = job;
    job.status = JobStatus::Completed;
    job.completed_at = Some(chrono::Utc::now());
    job.result = Some(serde_json::json!({
        "model": model,
        "item": plan_item,
        "itemFilePath": item_file_path.as_ref().map(|p| p.display().to_string()),
        "rawChars": accumulated.len(),
        "usage": { "inputTokens": usage_in, "outputTokens": usage_out },
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!("parsed plan item: {}", plan_item.id)),
        delta: None,
    })
    .await;
}

/// 把 PlanItem 写到 `<items_dir>/<sanitized id>.json`。
/// 用 tempfile + rename 模式避免半截文件；id 经 sanitize_entity_name 兜底，
/// 防止恶意 / 中文 id 串目录。
async fn persist_plan_item(
    items_dir: &std::path::Path,
    plan_item: &PlanItem,
) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(items_dir)
        .await
        .map_err(|e| format!("create items dir: {e}"))?;
    let safe_id = super::code_generate::sanitize_entity_name(&plan_item.id);
    let final_path = items_dir.join(format!("{safe_id}.json"));
    let tmp_path = items_dir.join(format!("{safe_id}.json.tmp"));
    let body =
        serde_json::to_vec_pretty(plan_item).map_err(|e| format!("serialize plan item: {e}"))?;
    tokio::fs::write(&tmp_path, &body)
        .await
        .map_err(|e| format!("write tmp: {e}"))?;
    tokio::fs::rename(&tmp_path, &final_path)
        .await
        .map_err(|e| format!("rename to {}: {e}", final_path.display()))?;
    Ok(final_path)
}

fn build_user_prompt(request: &SubmitSingleAssetPlanRequest) -> String {
    let mut buf = String::new();
    buf.push_str("需求描述：\n");
    buf.push_str(request.requirements.trim());
    buf.push_str("\n\n");
    if let Some(t) = &request.asset_type
        && !t.trim().is_empty()
    {
        buf.push_str(&format!("用户指定资产类型：{}\n", t.trim()));
    }
    buf.push_str("请按 schema 输出严格 JSON 对象，不要附加任何说明文字。\n");
    buf
}

/// 解析 LLM 响应为 PlanItem。
///
/// 容错顺序：
/// 1. 尝试整体作为 JSON 对象
/// 2. 若失败，尝试剥离 ``` fence 后再解
/// 3. 若仍失败，尝试 strip 到第一个 `{` 与最后一个 `}` 之间
pub(crate) fn parse_plan_item(raw: &str) -> Result<PlanItem, String> {
    let mut last_err = String::new();
    if let Ok(p) = serde_json::from_str::<PlanItem>(raw.trim()) {
        return Ok(p);
    }
    if let Some(inner) = extract_first_code_block(raw) {
        if let Ok(p) = serde_json::from_str::<PlanItem>(inner.trim()) {
            return Ok(p);
        }
        last_err = format!(
            "code-block extraction failed: {}",
            serde_json::from_str::<PlanItem>(inner.trim())
                .unwrap_err()
        );
    }
    if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}'))
        && end > start
    {
        let candidate = &raw[start..=end];
        if let Ok(p) = serde_json::from_str::<PlanItem>(candidate) {
            return Ok(p);
        }
        last_err = format!(
            "braces extraction failed: {}",
            serde_json::from_str::<PlanItem>(candidate)
                .unwrap_err()
        );
    } else if last_err.is_empty() {
        last_err = format!(
            "direct parse failed: {}",
            serde_json::from_str::<PlanItem>(raw.trim())
                .unwrap_err()
        );
    }
    Err(format!(
        "PlanItem parse failed — {last_err} — raw(500): {}",
        truncate(raw, 500)
    ))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}...[truncated]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmClient, LlmError,
        Usage,
    };
    use crate::platform::application::JobApplicationService;
    use crate::platform::domain::JobRepository;
    use crate::platform::infra::FileJobRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;

    struct ScriptedLlm {
        events: Mutex<Vec<Result<StreamEvent, LlmError>>>,
    }

    #[async_trait]
    impl LlmClient for ScriptedLlm {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            unimplemented!()
        }
        async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
            let evs: Vec<_> = self.events.lock().unwrap().drain(..).collect();
            Ok(Box::pin(stream::iter(evs)))
        }
    }

    fn one_chunk(text: &str) -> Vec<Result<StreamEvent, LlmError>> {
        vec![
            Ok(StreamEvent::Start {
                model: "plan-model".into(),
            }),
            Ok(StreamEvent::Delta {
                text: text.to_string(),
            }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::EndTurn,
                usage: Usage {
                    input_tokens: 100,
                    output_tokens: 50,
                },
            }),
        ]
    }

    async fn wait_terminal(service: &JobApplicationService, id: &JobId) {
        for _ in 0..100 {
            let job = service.get(id).await.unwrap();
            if job.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    const VALID_JSON: &str = r#"{
        "id": "flame_relic_v1",
        "type": "relic",
        "name": "Eternal Flame",
        "name_zhs": "永燃之火",
        "description": "At the start of combat, gain 3 Strength.",
        "goal": "为战士提供爆发起手",
        "detailed_description": "战斗开始时获得 3 力量，前两回合每回合受到 2 点伤害。",
        "implementation_notes": "OnBattleStart hook 加 StrengthPower；前两回合 OnPlayerTurnStart 扣血。",
        "needs_image": true,
        "image_description": "燃烧的火焰封印浮在金色基座上",
        "depends_on_item_ids": [],
        "scope_boundary": "不影响敌人；仅 player",
        "relationship_reason": "",
        "acceptance_notes": "进入战斗即获得 3 力量；前两回合可见受伤动画",
        "affected_targets": ["player"],
        "relationship_type": "independent",
        "clarification_status": "",
        "clarification_questions": [],
        "provided_image_b64": ""
    }"#;

    #[tokio::test]
    async fn single_asset_plan_parses_bare_json() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(one_chunk(VALID_JSON)),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitSingleAssetPlanRequest {
            requirements: "做一个开战获得力量的遗物".into(),
            asset_type: Some("relic".into()),
            max_tokens: None,
        };
        let id = service
            .submit_single_asset_plan(req, None, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.expect("result");
        assert_eq!(res["item"]["id"], "flame_relic_v1");
        assert_eq!(res["item"]["type"], "relic");
        assert_eq!(res["item"]["needs_image"], true);
    }

    #[tokio::test]
    async fn single_asset_plan_parses_fenced_json() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();

        let wrapped =
            format!("当然，以下是您的方案：\n\n```json\n{VALID_JSON}\n```\n\n希望有帮助！");

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(one_chunk(&wrapped)),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitSingleAssetPlanRequest {
            requirements: "测试 fence 容错".into(),
            asset_type: None,
            max_tokens: None,
        };
        let id = service
            .submit_single_asset_plan(req, None, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.unwrap();
        assert_eq!(res["item"]["id"], "flame_relic_v1");
    }

    #[tokio::test]
    async fn single_asset_plan_extracts_json_from_prose() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();

        let prose = format!("好的，我给你一个建议方案。\n\n{VALID_JSON}\n\n如果需要调整告诉我。");

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(one_chunk(&prose)),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitSingleAssetPlanRequest {
            requirements: "需求".into(),
            asset_type: None,
            max_tokens: None,
        };
        let id = service
            .submit_single_asset_plan(req, None, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn single_asset_plan_fails_on_invalid_json() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(one_chunk("抱歉我不会输出 JSON。")),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitSingleAssetPlanRequest {
            requirements: "需求".into(),
            asset_type: None,
            max_tokens: None,
        };
        let id = service
            .submit_single_asset_plan(req, None, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.unwrap_or_default().contains("parse plan json"));
    }

    #[tokio::test]
    async fn single_asset_plan_fails_on_empty_requirements() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(vec![]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitSingleAssetPlanRequest {
            requirements: "   ".into(),
            ..Default::default()
        };
        let id = service
            .submit_single_asset_plan(req, None, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .unwrap_or_default()
                .contains("requirements is empty")
        );
    }

    #[tokio::test]
    async fn single_asset_plan_writes_items_file_when_dir_provided() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let items = td.path().join("items");

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(one_chunk(VALID_JSON)),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitSingleAssetPlanRequest {
            requirements: "测试 items 落盘".into(),
            asset_type: Some("relic".into()),
            max_tokens: None,
        };
        let id = service
            .submit_single_asset_plan(req, Some(items.clone()), sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);

        let expected = items.join("flame_relic_v1.json");
        assert!(
            expected.is_file(),
            "items file not written: {}",
            expected.display()
        );

        let body = std::fs::read_to_string(&expected).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["id"], "flame_relic_v1");
        assert_eq!(v["needs_image"], true);

        let res = job.result.unwrap();
        assert!(
            res["itemFilePath"]
                .as_str()
                .unwrap_or("")
                .ends_with("flame_relic_v1.json"),
            "result.itemFilePath should point at the new file: {:?}",
            res["itemFilePath"]
        );
    }

    #[test]
    fn parse_plan_item_handles_unicode_quotes_in_string_fields() {
        // 中文引号/省略号要能保留
        let json_with_unicode = VALID_JSON.replace("永燃之火", "永燃之火（火之传说……）");
        let item = parse_plan_item(&json_with_unicode).expect("should parse");
        assert!(item.name_zhs.contains("火之传说"));
    }

    #[test]
    fn build_user_prompt_omits_blank_asset_type() {
        let req = SubmitSingleAssetPlanRequest {
            requirements: "x".into(),
            asset_type: Some("   ".into()),
            ..Default::default()
        };
        let p = build_user_prompt(&req);
        assert!(!p.contains("用户指定资产类型"));
    }

    #[test]
    fn planner_prompt_forbids_unsourced_hook_names() {
        assert!(SYSTEM_PROMPT.contains("不得猜测具体 hook 名"));
        assert!(SYSTEM_PROMPT.contains("当前源码检索官方相似实现和生命周期调用方"));
        assert!(SYSTEM_PROMPT.contains("不要发明 OnCombatStart、OnPlayerStartTurn"));
    }
}
