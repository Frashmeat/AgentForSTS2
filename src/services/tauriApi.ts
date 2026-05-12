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

export interface ReadinessFlags {
  llmConfigured: boolean;
  imageGenConfigured: boolean;
  activeProjectOpen: boolean;
  /// ML rembg 预热是否就绪（feature ml-rembg + 模型加载成功）
  imageProcReady: boolean;
  /// 后台任务 worker 是否在跑（Stage 3.4 Web 轨上线前 desktop 始终为 true）
  queueWorkerReady: boolean;
}

export interface HealthReport {
  status: "ok" | "degraded";
  role: Role;
  coreVersion: string;
  serverTime: string;
  config: ConfigStatus;
  readiness: ReadinessFlags;
}

export function getHealth(): Promise<HealthReport> {
  return invoke<HealthReport>("get_health");
}

// -------- Local Capabilities --------

export interface LocalCapabilities {
  os: string;
  arch: string;
  cpuCount: number;
  ilspycmdFound: boolean;
  ilspycmdPath: string | null;
  dotnetVersion: string | null;
  warnings: string[];
}

export function getLocalCapabilitiesSync(): Promise<LocalCapabilities> {
  return invoke<LocalCapabilities>("get_local_capabilities_sync");
}

export function getLocalCapabilitiesFull(): Promise<LocalCapabilities> {
  return invoke<LocalCapabilities>("get_local_capabilities_full");
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

export interface ExportPackStats {
  outputPath: string;
  zipBytes: number;
  gameFiles: number;
  baselibIncluded: boolean;
}

export interface ImportPackStats {
  gameFilesWritten: number;
  baselibWritten: boolean;
  manifestReplaced: boolean;
}

export function exportKnowledgePack(
  outputPath: string,
  machineHint?: string,
): Promise<ExportPackStats> {
  return invoke<ExportPackStats>("export_knowledge_pack", {
    outputPath,
    machineHint: machineHint ?? null,
  });
}

export function importKnowledgePack(
  inputPath: string,
  overwrite: boolean,
): Promise<ImportPackStats> {
  return invoke<ImportPackStats>("import_knowledge_pack", {
    inputPath,
    overwrite,
  });
}

// -------- mod_analyzer --------

export interface CsprojSummary {
  targetFramework: string | null;
  sdk: string | null;
  packageReferences: string[];
}

export interface ModMeta {
  name: string | null;
  author: string | null;
  version: string | null;
  rawExcerpt: string;
}

export interface ModAnalysisReport {
  projectRoot: string;
  csprojPath: string | null;
  csprojSummary: CsprojSummary | null;
  modMeta: ModMeta | null;
  csFilesCount: number;
  csTotalBytes: number;
  artifactsCount: number;
  warnings: string[];
}

export function analyzeModProject(projectRoot: string): Promise<ModAnalysisReport> {
  return invoke<ModAnalysisReport>("analyze_mod_project", { projectRoot });
}

// -------- Image proc prewarm --------

export type PrewarmStatus =
  | { state: "idle" }
  | { state: "loading"; message: string }
  | { state: "ready"; model: string }
  | { state: "failed"; message: string };

export function imageProcStatus(): Promise<PrewarmStatus> {
  return invoke<PrewarmStatus>("image_proc_status");
}

// -------- Audit + PlanArtifact --------

export interface AuditEntry {
  timestamp: string;
  kind: string;
  message: string;
  refId?: string | null;
  data?: unknown;
}

export type ArtifactState =
  | "pending"
  | "in_progress"
  | "generated"
  | "reviewed"
  | "failed";

export interface ArtifactStatus {
  itemId: string;
  state: ArtifactState;
  updatedAt: string;
  lastJobId?: string | null;
  csPath?: string | null;
  pngPath?: string | null;
  note?: string | null;
}

export function auditAppend(
  kind: string,
  message: string,
  refId?: string,
  data?: unknown,
): Promise<void> {
  return invoke<void>("audit_append", {
    kind,
    message,
    refId: refId ?? null,
    data: data ?? null,
  });
}

export function auditReadRecent(limit: number): Promise<AuditEntry[]> {
  return invoke<AuditEntry[]>("audit_read_recent", { limit });
}

export function planArtifactSave(status: ArtifactStatus): Promise<void> {
  return invoke<void>("plan_artifact_save", { status });
}

export function planArtifactLoad(itemId: string): Promise<ArtifactStatus | null> {
  return invoke<ArtifactStatus | null>("plan_artifact_load", { itemId });
}

export function planArtifactList(): Promise<ArtifactStatus[]> {
  return invoke<ArtifactStatus[]>("plan_artifact_list");
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
  csharp_name: string;
  created_at: string;
  schema_version: number;
  sts2_path: string | null;
  template_version: string | null;
  scaffolded: boolean;
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
  | "log_analysis"
  | "knowledge_refresh";

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

export type SubmitCodeGenerateRequest =
  | { mode: "asset"; request: AssetCodegenRequest }
  | { mode: "custom_code"; request: CustomCodegenRequest };

export interface SubmitBuildProjectRequest {
  project_root: string;
  max_attempts: number;
}

export function submitCodeGenerateJob(
  request: SubmitCodeGenerateRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_code_generate_job", { request });
}

export function submitBuildProjectJob(
  request: SubmitBuildProjectRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_build_project_job", { request });
}

// -------- Phase B/2.2.x Job kinds（stage 3.5 第二轮 + 2.2.1 落地） --------

export interface SubmitLogAnalysisRequest {
  log_path?: string | null;
  log_text?: string | null;
  context_hint?: string | null;
  max_log_chars?: number | null;
}

export function submitLogAnalysisJob(
  request: SubmitLogAnalysisRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_log_analysis_job", { request });
}

export interface SubmitPackageProjectRequest {
  source_dir: string;
  output_path?: string | null;
  compression_level?: number | null;
}

export function submitPackageProjectJob(
  request: SubmitPackageProjectRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_package_project_job", { request });
}

export interface SubmitBatchCustomCodeRequest {
  items: CustomCodegenRequest[];
  fail_fast?: boolean;
}

export function submitBatchCustomCodeJob(
  request: SubmitBatchCustomCodeRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_batch_custom_code_job", { request });
}

export interface SubmitSingleAssetPlanRequest {
  requirements: string;
  asset_type?: string | null;
  max_tokens?: number | null;
}

export function submitSingleAssetPlanJob(
  request: SubmitSingleAssetPlanRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_single_asset_plan_job", { request });
}

export interface SubmitKnowledgeRefreshRequest {
  sts2_dll_path: string;
  ilspycmd_path?: string | null;
  force?: boolean;
  include_baselib?: boolean;
}

export function submitKnowledgeRefreshJob(
  request: SubmitKnowledgeRefreshRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_knowledge_refresh_job", { request });
}

export interface SubmitAssetGenerateRequest {
  asset_request: AssetCodegenRequest;
  image_prompt?: string | null;
  image_size?: string | null;
}

export function submitAssetGenerateJob(
  request: SubmitAssetGenerateRequest,
): Promise<SubmitJobAck> {
  return invoke<SubmitJobAck>("submit_asset_generate_job", { request });
}
