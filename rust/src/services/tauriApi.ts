// Tauri 端 API 实现 —— 签名权威，webApi.ts 必须实现同名同签名的所有导出。

import { invoke } from "@tauri-apps/api/core";

// -------- Health --------

export type Role = "web" | "workstation";

export interface ConfigStatus {
  path: string | null;
  filePresent: boolean;
  loaded: boolean;
  errors: string[];
}

export interface HealthReport {
  status: "ok" | "degraded";
  role: Role;
  coreVersion: string;
  serverTime: string;
  config: ConfigStatus;
}

export function getHealth(): Promise<HealthReport> {
  return invoke<HealthReport>("get_health");
}

// -------- Knowledge --------

export type OverallState = "fresh" | "missing" | "stale";
export type SourceMode = "runtime_decompiled" | "missing";

export interface GameStatus {
  sourceMode: SourceMode;
  knowledgePath: string;
  hasDecompiledSources: boolean;
}

export interface BaselibStatus {
  sourceMode: SourceMode;
  knowledgePath: string;
  hasDecompiledSources: boolean;
}

export interface KnowledgeStatus {
  overall: OverallState;
  knowledgeRoot: string;
  warnings: string[];
  game: GameStatus;
  baselib: BaselibStatus;
  embeddedTemplates: string[];
}

export function getKnowledgeStatus(): Promise<KnowledgeStatus> {
  return invoke<KnowledgeStatus>("get_knowledge_status");
}

export function checkKnowledgeStatus(): Promise<KnowledgeStatus> {
  return invoke<KnowledgeStatus>("check_knowledge_status");
}

// -------- Planning --------

export type AssetItemType =
  | "card"
  | "card_fullscreen"
  | "relic"
  | "power"
  | "character"
  | "custom_code";

export type ReviewStrictness = "efficient" | "balanced" | "strict";

export type PlanItemReviewStatus = "clear" | "needs_user_input" | "invalid";

// Mirror of ats-core::planning::PlanItem. All fields optional on input — the
// Rust side fills defaults — but the response always carries every field.
export interface PlanItem {
  id: string;
  type: AssetItemType;
  name: string;
  name_zhs?: string;
  description?: string;
  goal?: string;
  detailed_description?: string;
  implementation_notes?: string;
  needs_image?: boolean;
  image_description?: string;
  depends_on_item_ids?: string[];
  scope_boundary?: string;
  relationship_reason?: string;
  acceptance_notes?: string;
  affected_targets?: string[];
  relationship_type?: string;
  clarification_status?: string;
  clarification_questions?: string[];
  provided_image_b64?: string;
}

export interface ModPlan {
  mod_name: string;
  summary: string;
  items: PlanItem[];
}

export interface PlanValidationIssue {
  code: string;
  message: string;
  field: string;
}

export interface PlanItemValidation {
  itemId: string;
  status: PlanItemReviewStatus;
  issues: PlanValidationIssue[];
  missingFields: string[];
  clarificationQuestions: string[];
}

export interface PlanValidationResult {
  strictness: ReviewStrictness;
  items: PlanItemValidation[];
}

export function validatePlan(
  plan: ModPlan,
  strictness: ReviewStrictness = "balanced",
): Promise<PlanValidationResult> {
  return invoke<PlanValidationResult>("validate_plan_cmd", { plan, strictness });
}

export type BundleReviewStatus = "clear" | "needs_confirmation" | "split_recommended";
export type BundleDecision =
  | "unresolved"
  | "accepted"
  | "split_requested"
  | "needs_item_revision";

export interface DependencyGroup {
  itemIds: string[];
}

export interface RiskDetail {
  code: string;
  title: string;
  summary: string;
  recommendation: string;
  impact?: string;
}

export interface RecommendedAction {
  action: string;
  label: string;
  description: string;
  emphasis: string;
}

export interface ExecutionBundle {
  bundleId: string;
  itemIds: string[];
  status: BundleReviewStatus;
  reason: string;
  riskCodes: string[];
  riskDetails: RiskDetail[];
  recommendedActions: RecommendedAction[];
  blockingReason: string;
}

export interface ExecutionPlanPreview {
  strictness: ReviewStrictness;
  dependencyGroups: DependencyGroup[];
  executionBundles: ExecutionBundle[];
}

export function buildExecutionPlan(
  plan: ModPlan,
  strictness: ReviewStrictness = "balanced",
  bundleDecisions: Record<string, BundleDecision> = {},
): Promise<ExecutionPlanPreview> {
  return invoke<ExecutionPlanPreview>("build_execution_plan_cmd", {
    plan,
    strictness,
    bundleDecisions,
  });
}

// -------- Codegen --------

export interface AssetCodegenRequest {
  design_description: string;
  asset_type: string;
  asset_name: string;
  image_paths: string[];
  project_root: string;
  name_zhs: string;
  skip_build: boolean;
}

export interface CustomCodegenRequest {
  description: string;
  implementation_notes: string;
  name: string;
  project_root: string;
  skip_build: boolean;
}

export interface AssetGroupItem {
  item: PlanItem;
  image_paths: string[];
}

export interface AssetGroupRequest {
  assets: AssetGroupItem[];
  project_root: string;
}

export interface ModProjectRequest {
  project_name: string;
  target_dir: string;
}

export function codegenAssetPrompt(request: AssetCodegenRequest): Promise<string> {
  return invoke<string>("codegen_asset_prompt", { request });
}

export function codegenCustomCodePrompt(
  request: CustomCodegenRequest,
): Promise<string> {
  return invoke<string>("codegen_custom_code_prompt", { request });
}

export function codegenAssetGroupPrompt(
  request: AssetGroupRequest,
): Promise<string> {
  return invoke<string>("codegen_asset_group_prompt", { request });
}

export function codegenBuildPrompt(maxAttempts = 3): Promise<string> {
  return invoke<string>("codegen_build_prompt", { maxAttempts });
}

export function codegenCreateModProjectPrompt(
  request: ModProjectRequest,
): Promise<string> {
  return invoke<string>("codegen_create_mod_project_prompt", { request });
}

export function codegenPackagePrompt(): Promise<string> {
  return invoke<string>("codegen_package_prompt");
}

// -------- LLM --------

export type MessageRole = "system" | "user" | "assistant";
export type FinishReason =
  | "end_turn"
  | "max_tokens"
  | "stop_sequence"
  | "tool_use"
  | "other";

export interface LlmMessage {
  role: MessageRole;
  content: string;
}

export interface CompletionRequest {
  messages: LlmMessage[];
  system_prompt?: string | null;
  max_tokens: number;
  temperature?: number | null;
  model?: string | null;
}

export interface Usage {
  inputTokens: number;
  outputTokens: number;
}

export interface CompletionResponse {
  model: string;
  content: string;
  finishReason: FinishReason;
  usage: Usage;
}

export type StreamEvent =
  | { kind: "start"; model: string }
  | { kind: "delta"; text: string }
  | { kind: "end"; finishReason: FinishReason; usage: Usage };

export function llmComplete(request: CompletionRequest): Promise<CompletionResponse> {
  return invoke<CompletionResponse>("llm_complete", { request });
}

/**
 * 启动流式补全。后端通过 Tauri 事件 `llm-stream` 推送 chunk，前端用
 * `@tauri-apps/api/event` 的 listen 接收，按 request_id 过滤。
 *
 * 调用方传入唯一 request_id（uuid 或时间戳），命令本身立刻返回；
 * 实际数据通过事件流到达。Web 端 webApi 走 SSE 实现相同语义。
 */
export function llmStartStream(
  requestId: string,
  request: CompletionRequest,
): Promise<void> {
  return invoke<void>("llm_start_stream", { requestId, request });
}

/** Tauri 事件 payload —— 与 src-tauri/src/commands/llm.rs::StreamPayload 一致 */
export type LlmStreamPayload =
  | { type: "event"; request_id: string; event: StreamEvent }
  | { type: "error"; request_id: string; message: string }
  | { type: "done"; request_id: string };

// -------- Project（仅桌面端）--------

export interface ProjectMeta {
  name: string;
  created_at: string;
  schema_version: number;
  sts2_path: string | null;
  template_version: string | null;
}

export interface ProjectSnapshot {
  path: string;
  meta: ProjectMeta;
}

export interface RecentEntry {
  path: string;
  name: string;
  last_opened_at: string;
}

export function listRecentProjects(): Promise<RecentEntry[]> {
  return invoke<RecentEntry[]>("list_recent_projects");
}

export function createProject(
  parentDir: string,
  name: string,
): Promise<ProjectSnapshot> {
  return invoke<ProjectSnapshot>("create_project", { parentDir, name });
}

export function openProject(path: string): Promise<ProjectSnapshot> {
  return invoke<ProjectSnapshot>("open_project", { path });
}

export function closeProject(): Promise<void> {
  return invoke<void>("close_project");
}

export function currentProject(): Promise<ProjectSnapshot | null> {
  return invoke<ProjectSnapshot | null>("current_project");
}

export function forgetRecentProject(path: string): Promise<void> {
  return invoke<void>("forget_recent_project", { path });
}

// -------- Platform Jobs --------

export type JobKind =
  | "text_generate"
  | "code_generate"
  | "asset_generate"
  | "batch_custom_code"
  | "build_project"
  | "package_project"
  | "single_asset_plan"
  | "log_analysis";

export type JobStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export interface JobProgressFields {
  stage: string;
  percent: number | null;
  message: string | null;
}

export interface Job {
  id: string;
  kind: JobKind;
  status: JobStatus;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  payload: unknown;
  result: unknown;
  error: string | null;
  progress: JobProgressFields | null;
  attempts: number;
}

export interface JobSummary {
  id: string;
  kind: JobKind;
  status: JobStatus;
  createdAt: string;
  completedAt: string | null;
  progress: JobProgressFields | null;
  error: string | null;
}

export interface SubmitTextGenerateRequest {
  prompt: string;
  system_prompt?: string | null;
  max_tokens?: number | null;
  temperature?: number | null;
  model?: string | null;
}

export interface SubmitJobAck {
  jobId: string;
}

/** ProgressSink::emit 推送的事件（job-progress） */
export interface JobProgressEvent {
  jobId: string;
  stage: string;
  percent: number | null;
  message: string | null;
  delta: string | null;
}

export function submitTextGenerateJob(
  request: SubmitTextGenerateRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_text_generate_job", { request });
}

export function getJob(id: string): Promise<Job> {
  return invoke<Job>("get_job", { id });
}

export function listJobs(): Promise<JobSummary[]> {
  return invoke<JobSummary[]>("list_jobs");
}

export function cancelJob(id: string): Promise<void> {
  return invoke<void>("cancel_job", { id });
}
