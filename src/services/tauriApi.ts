import { invoke } from "@tauri-apps/api/core";

import { toActionableFailure } from "./actionableFailure";
import { buildFeatureSubmission } from "./featureSubmission";
export { isActionableFailure, toActionableFailure } from "./actionableFailure";
export type { ActionableFailure, RecoveryAction } from "./actionableFailure";

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw toActionableFailure(error);
  }
}

export type BuildVariant = "development" | "baseline" | "ml";
export interface BuildInfo {
  commit: string;
  variant: BuildVariant;
  features: string[];
  buildId: string;
}

export interface HealthReport {
  status: string;
  role?: "web" | "workstation";
  build?: BuildInfo | null;
  gamePackId?: string | null;
  gamePackSha256?: string | null;
  featureCount: number;
  projectOpen?: boolean;
  truthReady?: boolean;
  mediaGenerationRegistered?: boolean;
  projectExecutionAvailable?: boolean;
  llmConfigured?: boolean;
  imageGenerationConfigured?: boolean;
  configLoaded?: boolean;
  configErrors?: string[];
}

export function getHealth(): Promise<HealthReport> {
  return invokeCommand<HealthReport>("get_health");
}

export interface SchemaRef {
  id: string;
  version: number;
}

export interface VersionedPayload<T extends Record<string, unknown> = Record<string, unknown>> {
  schema: SchemaRef;
  payload: T;
}

export interface FeatureContract {
  id: string;
  requestSchema: SchemaRef;
  resultSchema: SchemaRef;
  requiredContributions: string[];
}

export function getFeatureCatalog(): Promise<FeatureContract[]> {
  return invokeCommand<FeatureContract[]>("get_feature_catalog");
}

export type RunStatus = "pending" | "running" | "succeeded" | "failed" | "cancelled";
export type CancellationReason = "user" | "project_close" | "project_switch" | "app_shutdown";
export type RunTimelineEventKind =
  | "created"
  | "started"
  | "cancel_requested"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "interrupted";

export interface RunProgress {
  stage: string;
  percent?: number | null;
  message?: string | null;
}

export interface RunFailure {
  code: string;
  stage: string;
  details?: VersionedPayload | null;
}

export interface RunTimelineEvent {
  kind: RunTimelineEventKind;
  at: string;
  stage?: string | null;
  failureCode?: string | null;
  cancellationReason?: CancellationReason | null;
}

export interface RunRecord {
  schemaVersion: 3;
  id: string;
  featureId: string;
  status: RunStatus;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  request: VersionedPayload;
  progress: RunProgress | null;
  failure: RunFailure | null;
  result: VersionedPayload | null;
  attempts: number;
  timeline: RunTimelineEvent[];
}

export interface RunSummary {
  id: string;
  featureId: string;
  status: RunStatus;
  createdAt: string;
  completedAt: string | null;
  progress: RunProgress | null;
  failure: RunFailure | null;
}

function isVersionedPayload(value: unknown): value is VersionedPayload {
  if (!isRecord(value) || !isRecord(value.schema) || !isRecord(value.payload)) return false;
  return (
    typeof value.schema.id === "string" &&
    Number.isInteger(value.schema.version) &&
    (value.schema.version as number) > 0
  );
}

function isRunRecord(value: unknown): value is RunRecord {
  if (!isRecord(value)) return false;
  return (
    value.schemaVersion === 3 &&
    typeof value.id === "string" &&
    typeof value.featureId === "string" &&
    isRunStatus(value.status) &&
    isVersionedPayload(value.request) &&
    (value.result === null || isVersionedPayload(value.result)) &&
    (value.failure === null || isRunFailure(value.failure)) &&
    Array.isArray(value.timeline)
  );
}

export async function getRun(runId: string): Promise<RunRecord> {
  const value = await invokeCommand<unknown>("get_run", { runId });
  if (!isRunRecord(value)) throw toActionableFailure(undefined);
  return value;
}

export async function listRuns(): Promise<RunSummary[]> {
  const values = await invokeCommand<unknown>("list_runs");
  if (!Array.isArray(values) || !values.every(isRunSummary)) {
    throw toActionableFailure(undefined);
  }
  return values;
}

export function cancelRun(runId: string): Promise<boolean> {
  return invokeCommand<boolean>("cancel_run", { runId });
}

export interface PlanItem extends Record<string, unknown> {
  itemId: string;
  itemType: string;
  name: string;
  summary: string;
  behaviorIntent: string[];
  implementationConstraints: string[];
  evidenceRequirements: string[];
  requiredResourceRoles: string[];
  acceptanceCriteria: string[];
}

export interface ModPlanRequest extends Record<string, unknown> {
  requirements: string;
  itemType?: string | null;
}

export interface SelectedResource extends Record<string, unknown> {
  resourceId: string;
  selectedVersion: string;
}

export interface SingleGenerateRequest extends Record<string, unknown> {
  artifactId: string;
  modId: string;
  plan: PlanItem;
  selectedResources: SelectedResource[];
}

export interface BatchGenerateRequest extends Record<string, unknown> {
  items: SingleGenerateRequest[];
  failFast: boolean;
}

export interface ComplexPlanningItem extends Record<string, unknown> {
  request: ModPlanRequest;
  artifactId: string;
  selectedResources: SelectedResource[];
}

export interface ProjectPackageRequest extends Record<string, unknown> {
  artifactId: string;
  modId: string;
  sourceRelativeRoot: string;
  outputRelativePath: string;
  compressionLevel?: number | null;
}

export interface ComplexGenerateRequest extends Record<string, unknown> {
  modId: string;
  planningItems: ComplexPlanningItem[];
  failFast: boolean;
  package: ProjectPackageRequest;
}

export interface LogAnalyzeRequest extends Record<string, unknown> {
  logText: string;
  contextHint?: string | null;
  maxLogChars?: number | null;
}

export interface ResourcePrepareRequest extends Record<string, unknown> {
  logicalRole: string;
  mediaType: string;
  source:
    | { kind: "user_upload" }
    | { kind: "pack_default" }
    | { kind: "ai_generated"; prompt: string; fileName: string; model?: string | null };
}

export interface ProjectBuildRequest extends Record<string, unknown> {}

export function submitModPlan(request: ModPlanRequest): Promise<string> {
  return submit("mod.plan", "feature.mod-plan-request", request);
}

export function submitSingleGenerate(request: SingleGenerateRequest): Promise<string> {
  return submit("mod.generate.single", "feature.mod-generate-single-request", request, 2);
}

export function submitBatchGenerate(request: BatchGenerateRequest): Promise<string> {
  return submit("mod.generate.batch", "feature.mod-generate-batch-request", request, 2);
}

export function submitComplexGenerate(request: ComplexGenerateRequest): Promise<string> {
  return submit("mod.generate.complex", "feature.mod-generate-complex-request", request);
}

export function submitLogAnalyze(request: LogAnalyzeRequest): Promise<string> {
  return submit("log.analyze", "feature.log-analyze-request", request);
}

export function submitResourcePrepare(
  request: ResourcePrepareRequest,
  sourcePath?: string,
): Promise<string> {
  return submit("resource.prepare", "feature.resource-prepare-request", request, 1, sourcePath);
}

export function submitProjectBuild(request: ProjectBuildRequest = {}): Promise<string> {
  return submit("project.build", "feature.project-build-request", request);
}

export function submitProjectPackage(request: ProjectPackageRequest): Promise<string> {
  return submit("project.package", "feature.project-package-request", request);
}

function submit(
  featureId: string,
  schemaId: string,
  payload: Record<string, unknown>,
  schemaVersion = 1,
  sourcePath?: string,
): Promise<string> {
  return invokeCommand<string>("submit_feature", {
    submission: buildFeatureSubmission(featureId, schemaId, payload, schemaVersion, sourcePath),
  });
}

export interface CurrentProject {
  path: string;
  name: string;
  csharpName: string;
  gameId: string;
  closing: boolean;
}

export interface RecentEntry {
  path: string;
  name: string;
  last_opened_at: string;
}

export function listRecentProjects(): Promise<RecentEntry[]> {
  return invokeCommand<RecentEntry[]>("list_recent_projects");
}
export function createProject(parentDir: string, name: string): Promise<CurrentProject> {
  return invokeCommand<CurrentProject>("create_project", { parentDir, name });
}
export function openProject(path: string): Promise<CurrentProject> {
  return invokeCommand<CurrentProject>("open_project", { path });
}
export function closeProject(): Promise<void> {
  return invokeCommand<void>("close_project");
}
export function currentProject(): Promise<CurrentProject | null> {
  return invokeCommand<CurrentProject | null>("current_project");
}
export function forgetRecentProject(path: string): Promise<void> {
  return invokeCommand<void>("forget_recent_project", { path });
}

export interface TruthStatus {
  ready: boolean;
  snapshotId: string | null;
}
export function getTruthStatus(): Promise<TruthStatus> {
  return invokeCommand<TruthStatus>("get_truth_status");
}
export function importTruth(): Promise<TruthStatus> {
  return invokeCommand<TruthStatus>("import_truth");
}

export interface LlmSnapshot {
  provider: string;
  model: string;
  baseUrl: string;
  customPrompt: string;
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
  githubTokenMasked: string;
}
export interface SettingsSnapshot {
  configPath: string | null;
  configLoaded: boolean;
  configErrors: string[];
  llm: LlmSnapshot;
  imageGen: ImageGenSnapshot;
  runtimeWorkstation: RuntimeSnapshot;
  runtimeWeb: RuntimeSnapshot;
  knowledge: { sts2DllPath: string };
  toolchain: { godotExePath: string };
}
export interface SettingsPatch {
  llm?: Partial<{
    provider: string;
    model: string;
    baseUrl: string;
    customPrompt: string;
    apiKey: string;
  }> | null;
  imageGen?: Partial<{
    provider: string;
    model: string;
    baseUrl: string;
    size: string;
    protocol: string;
    apiKey: string;
  }> | null;
  runtimeWorkstation?: Partial<{ githubToken: string }> | null;
  knowledge?: Partial<{ sts2DllPath: string }> | null;
  toolchain?: Partial<{ godotExePath: string }> | null;
}
export function getSettingsSnapshot(): Promise<SettingsSnapshot> {
  return invokeCommand<SettingsSnapshot>("get_settings_snapshot");
}
export function saveSettingsPatch(patch: SettingsPatch): Promise<SettingsSnapshot> {
  return invokeCommand<SettingsSnapshot>("save_settings_patch", { patch });
}
export function openConfigInEditor(): Promise<string> {
  return invokeCommand<string>("open_config_in_editor");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function isRunStatus(value: unknown): value is RunStatus {
  return ["pending", "running", "succeeded", "failed", "cancelled"].includes(String(value));
}
function isRunFailure(value: unknown): value is RunFailure {
  return isRecord(value) && typeof value.code === "string" && typeof value.stage === "string";
}
function isRunSummary(value: unknown): value is RunSummary {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.featureId === "string" &&
    isRunStatus(value.status)
  );
}
