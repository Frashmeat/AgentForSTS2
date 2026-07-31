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
  /** ML rembg 预热是否就绪（feature ml-rembg + 模型加载成功） */
  imageProcReady: boolean;
  /** 活动工程是否有经过完整校验的 current Truth Snapshot。 */
  truthSnapshotReady: boolean;
  /** 后台任务 worker 是否在跑（Stage 3.4 Web 轨上线前 desktop 始终为 true） */
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

// -------- Truth Snapshot --------

export type TruthSnapshotReadiness = "ready" | "missing" | "invalid";

export interface TruthSnapshotSource {
  id: string;
  kind: string;
  version: string | null;
  relativePath: string;
  sha256: string;
  sizeBytes: number;
}

export interface TruthSnapshotIndex {
  sourceId: string;
  indexer: string;
  provider: string;
  relativeRoot: string;
  fileCount: number;
  csFileCount: number;
  totalBytes: number;
  treeSha256: string;
}

export interface TruthSnapshotStatus {
  state: TruthSnapshotReadiness;
  gamePackId: string;
  snapshotId: string | null;
  createdAt: string | null;
  sources: TruthSnapshotSource[];
  indexes: TruthSnapshotIndex[];
  toolVersions: Record<string, string>;
  warnings: string[];
}

export function getTruthSnapshotStatus(): Promise<TruthSnapshotStatus> {
  return invoke<TruthSnapshotStatus>("get_truth_snapshot_status");
}

export function checkTruthSnapshotStatus(): Promise<TruthSnapshotStatus> {
  return invoke<TruthSnapshotStatus>("check_truth_snapshot_status");
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

// -------- Settings --------

export interface LlmSnapshot {
  provider: string;
  model: string;
  baseUrl: string;
  apiKeyMasked: string;
  apiKeyConfigured: boolean;
}

export interface ImageGenSnapshot {
  provider: string;
  model: string;
  baseUrl: string;
  size: string;
  protocol: string;
  apiKeyMasked: string;
  apiKeyConfigured: boolean;
}

export interface RuntimeSnapshot {
  host: string;
  port: number;
  mountFrontend: boolean;
  requiresDatabase: boolean;
  githubToken: string;
}
export interface SettingsSnapshot {
  configPath: string | null;
  configLoaded: boolean;
  configErrors: string[];
  llm: LlmSnapshot;
  imageGen: ImageGenSnapshot;
  runtimeWorkstation: RuntimeSnapshot;
  runtimeWeb: RuntimeSnapshot;
  knowledge: KnowledgeSnapshot;
  toolchain: ToolchainSnapshot;
}

export function getSettingsSnapshot(): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("get_settings_snapshot");
}

export function openConfigInEditor(): Promise<string> {
  return invoke<string>("open_config_in_editor");
}

export interface LlmPatch {
  provider?: string | null;
  model?: string | null;
  base_url?: string | null;
  api_key?: string | null;
}

export interface ImageGenPatch {
  provider?: string | null;
  model?: string | null;
  base_url?: string | null;
  size?: string | null;
  protocol?: string | null;
  api_key?: string | null;
}

export interface KnowledgeSnapshot {
  sts2DllPath: string;
}

export interface ToolchainSnapshot {
  godotExePath: string;
}

export interface RuntimePatch {
  github_token?: string | null;
}

export interface KnowledgePatch {
  sts2_dll_path?: string | null;
}

export interface ToolchainPatch {
  godot_exe_path?: string | null;
}

export interface SettingsPatch {
  llm?: LlmPatch | null;
  image_gen?: ImageGenPatch | null;
  runtime_workstation?: RuntimePatch | null;
  knowledge?: KnowledgePatch | null;
  toolchain?: ToolchainPatch | null;
}

export function saveSettingsPatch(patch: SettingsPatch): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("save_settings_patch", { patch });
}

export function discoverSts2Dll(): Promise<string | null> {
  return invoke<string | null>("discover_sts2_dll");
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

// -------- PlanArtifact --------

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
  lastRunId?: string | null;
  csPath?: string | null;
  pngPath?: string | null;
  note?: string | null;
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
  game_id: string;
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
  gameId: string,
): Promise<ProjectSnapshot> {
  return invoke<ProjectSnapshot>("create_project", { parentDir, name, gameId });
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

// -------- Platform Runs --------

export type RunKind =
  | "text_generate"
  | "code_generate"
  | "asset_generate"
  | "batch_custom_code"
  | "build_project"
  | "package_project"
  | "single_asset_plan"
  | "log_analysis"
  | "truth_snapshot_refresh";

export type RunStatus =
  | "pending"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface RunProgressFields {
  stage: string;
  percent: number | null;
  message: string | null;
}

export interface ActionableFailure {
  schemaVersion: number;
  code: string;
  stage: string;
  message: string;
  retryable: boolean;
  diagnosticRef?: string | null;
}

export type CancellationReason =
  | "user"
  | "project_close"
  | "project_switch"
  | "app_shutdown";

export type RunTimelineEventKind =
  | "created"
  | "started"
  | "cancel_requested"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "interrupted";

export interface RunTimelineEvent {
  kind: RunTimelineEventKind;
  at: string;
  stage?: string | null;
  failureCode?: string | null;
  cancellationReason?: CancellationReason | null;
}

export interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
}

export interface BatchArtifactItemResult {
  itemId: string;
  artifactManifestRef: string | null;
  manifestSha256: string | null;
  diagnosticRef: string | null;
}

export interface BuildStepResult {
  id: string;
  runner: string;
  success: boolean;
  exitCode: number;
  stdoutTail: string;
  stderrTail: string;
}

export type RunResult =
  | {
      kind: "text_generation";
      model: string;
      content: string;
      finishReason: string;
      usage: TokenUsage;
    }
  | {
      kind: "artifact_production";
      artifactManifestRef: string;
      manifestSha256: string;
      artifactId: string;
      entityName: string;
      model: string | null;
      usage: TokenUsage | null;
    }
  | {
      kind: "batch_artifact_production";
      total: number;
      succeeded: number;
      failed: number;
      items: BatchArtifactItemResult[];
    }
  | {
      kind: "build";
      projectRelativeRoot: string;
      steps: BuildStepResult[];
      artifactManifestRef: string | null;
      manifestSha256: string | null;
    }
  | {
      kind: "package";
      artifactManifestRef: string;
      manifestSha256: string;
      artifactId: string;
      filesAdded: number;
      uncompressedBytes: number;
      packageBytes: number;
    }
  | {
      kind: "plan";
      item: PlanItem;
      itemFileRef: string | null;
      model: string;
      usage: TokenUsage;
    }
  | {
      kind: "log_analysis";
      model: string;
      report: string;
      logChars: number;
      truncatedChars: number;
      usage: TokenUsage;
    }
  | {
      kind: "truth_snapshot_refresh";
      gamePackId: string;
      snapshotId: string;
      cacheHit: boolean;
      sourceCount: number;
      indexCount: number;
      toolVersions: Record<string, string>;
      warnings: string[];
    };

export interface RunRecord {
  schemaVersion: number;
  id: string;
  kind: RunKind;
  status: RunStatus;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  payload: unknown;
  progress: RunProgressFields | null;
  failure: ActionableFailure | null;
  result: RunResult | null;
  attempts: number;
  timeline: RunTimelineEvent[];
}

export interface RunSummary {
  id: string;
  kind: RunKind;
  status: RunStatus;
  createdAt: string;
  completedAt: string | null;
  progress: RunProgressFields | null;
  failure: ActionableFailure | null;
}

export interface SubmitTextGenerateRequest {
  prompt: string;
  system_prompt?: string | null;
  max_tokens?: number | null;
  temperature?: number | null;
  model?: string | null;
}

export interface SubmitRunAck {
  runId: string;
}

/** ProgressSink::emit 推送的事件（run-progress） */
export interface RunProgressEvent {
  runId: string;
  stage: string;
  percent: number | null;
  message: string | null;
  delta: string | null;
}

export function submitTextGenerateRun(
  request: SubmitTextGenerateRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_text_generate_run", { request });
}

export function getRun(id: string): Promise<RunRecord> {
  return invoke<RunRecord>("get_run", { id });
}

export function listRuns(): Promise<RunSummary[]> {
  return invoke<RunSummary[]>("list_runs");
}

export function cancelRun(id: string): Promise<void> {
  return invoke<void>("cancel_run", { id });
}

export type SubmitCodeGenerateRequest =
  | { mode: "asset"; request: AssetCodegenRequest }
  | { mode: "custom_code"; request: CustomCodegenRequest };

export interface SubmitBuildProjectRequest {
  project_root: string;
  max_attempts: number;
}

export function submitCodeGenerateRun(
  request: SubmitCodeGenerateRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_code_generate_run", { request });
}

export function submitBuildProjectRun(
  request: SubmitBuildProjectRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_build_project_run", { request });
}

// -------- Phase B/2.2.x RunRecord kinds（stage 3.5 第二轮 + 2.2.1 落地） --------

export interface SubmitLogAnalysisRequest {
  log_path?: string | null;
  log_text?: string | null;
  context_hint?: string | null;
  max_log_chars?: number | null;
}

export function submitLogAnalysisRun(
  request: SubmitLogAnalysisRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_log_analysis_run", { request });
}

export interface SubmitPackageProjectRequest {
  source_dir: string;
  output_path?: string | null;
  compression_level?: number | null;
}

export function submitPackageProjectRun(
  request: SubmitPackageProjectRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_package_project_run", { request });
}

export interface SubmitBatchCustomCodeRequest {
  items: CustomCodegenRequest[];
  fail_fast?: boolean;
}

export function submitBatchCustomCodeRun(
  request: SubmitBatchCustomCodeRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_batch_custom_code_run", { request });
}

export interface SubmitSingleAssetPlanRequest {
  requirements: string;
  asset_type?: string | null;
  max_tokens?: number | null;
}

export function submitSingleAssetPlanRun(
  request: SubmitSingleAssetPlanRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_single_asset_plan_run", { request });
}

export interface SubmitTruthSnapshotRefreshRequest {
  force?: boolean;
}

export function submitTruthSnapshotRefreshRun(
  request: SubmitTruthSnapshotRefreshRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_truth_snapshot_refresh_run", { request });
}

export interface SubmitAssetGenerateRequest {
  asset_request: AssetCodegenRequest;
  image_prompt?: string | null;
  image_size?: string | null;
}

export function submitAssetGenerateRun(
  request: SubmitAssetGenerateRequest,
): Promise<SubmitRunAck> {
  return invoke<SubmitRunAck>("submit_asset_generate_run", { request });
}
