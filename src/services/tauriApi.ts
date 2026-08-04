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

export type ItemFieldValueSpec =
  | { kind: "text"; multiline: boolean; minLength: number; maxLength: number }
  | { kind: "integer"; min: number; max: number }
  | { kind: "boolean" }
  | { kind: "choice"; options: ItemChoiceOption[] }
  | { kind: "string_list"; minItems: number; maxItems: number; itemMaxLength: number };

export interface ItemChoiceOption {
  value: string;
  displayNames: Record<string, string>;
}

export interface ItemFieldSpec {
  id: string;
  displayNames: Record<string, string>;
  required: boolean;
  value: ItemFieldValueSpec;
}

export interface ItemTypeDescriptor {
  id: string;
  displayNames: Record<string, string>;
  requiredLocales: string[];
  fields: ItemFieldSpec[];
  evidenceQueries: { symbols: string[]; terms: string[] }[];
  requiredResourceRoles: string[];
}

export type ItemCapabilityBlocker =
  | { code: "truth.snapshot_unavailable" }
  | { code: "truth.evidence_missing"; queryIndex: number };

export interface ItemTypeCapability {
  descriptor: ItemTypeDescriptor;
  ready: boolean;
  blockers: ItemCapabilityBlocker[];
}

export interface ItemCapabilityCatalog {
  gamePackId: string;
  gamePackSha256: string;
  truthSnapshotId?: string | null;
  itemTypes: ItemTypeCapability[];
}

export type ItemFieldValue =
  | { kind: "text"; value: string }
  | { kind: "integer"; value: number }
  | { kind: "boolean"; value: boolean }
  | { kind: "choice"; value: string }
  | { kind: "string_list"; value: string[] };

export interface ItemLocalization {
  name: string;
  description: string;
  status: "confirmed" | "outdated";
  translatedFrom?: string | null;
}

export interface ItemResourceBinding {
  resourceId: string;
  selectedVersion: string;
}

export interface ItemDefinition {
  schemaVersion: 1;
  itemId: string;
  itemType: string;
  canonicalFields: Record<string, ItemFieldValue>;
  behaviorIntent: string[];
  localizations: Record<string, ItemLocalization>;
  resourceBindings: Record<string, ItemResourceBinding>;
}

export interface StoredItemDefinition {
  definitionHash: string;
  definition: ItemDefinition;
}

export async function getItemCapabilities(): Promise<ItemCapabilityCatalog> {
  const value = await invokeCommand<unknown>("get_item_capabilities");
  if (!isItemCapabilityCatalog(value)) throw toActionableFailure(undefined);
  return value;
}

export async function listItemDefinitions(): Promise<StoredItemDefinition[]> {
  const value = await invokeCommand<unknown>("list_item_definitions");
  if (!Array.isArray(value) || !value.every(isStoredItemDefinition)) {
    throw toActionableFailure(undefined);
  }
  return value;
}

export async function getItemDefinition(
  itemId: string,
  definitionHash?: string,
): Promise<StoredItemDefinition> {
  const value = await invokeCommand<unknown>("get_item_definition", {
    itemId,
    definitionHash: definitionHash ?? null,
  });
  if (!isStoredItemDefinition(value)) throw toActionableFailure(undefined);
  return value;
}

export async function saveItemDefinition(
  definition: ItemDefinition,
): Promise<StoredItemDefinition> {
  const value = await invokeCommand<unknown>("save_item_definition", { definition });
  if (!isStoredItemDefinition(value)) throw toActionableFailure(undefined);
  return value;
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

function isItemCapabilityCatalog(value: unknown): value is ItemCapabilityCatalog {
  return (
    isRecord(value) &&
    typeof value.gamePackId === "string" &&
    isSha256(value.gamePackSha256) &&
    (value.truthSnapshotId === undefined || value.truthSnapshotId === null || isSha256(value.truthSnapshotId)) &&
    Array.isArray(value.itemTypes) &&
    value.itemTypes.every(isItemTypeCapability)
  );
}

function isItemTypeCapability(value: unknown): value is ItemTypeCapability {
  return (
    isRecord(value) &&
    typeof value.ready === "boolean" &&
    isItemTypeDescriptor(value.descriptor) &&
    Array.isArray(value.blockers) &&
    value.blockers.every(isItemCapabilityBlocker)
  );
}

function isItemCapabilityBlocker(value: unknown): value is ItemCapabilityBlocker {
  if (!isRecord(value) || typeof value.code !== "string") return false;
  return value.code === "truth.snapshot_unavailable"
    ? Object.keys(value).length === 1
    : value.code === "truth.evidence_missing" &&
        Object.keys(value).length === 2 &&
        Number.isInteger(value.queryIndex) &&
        (value.queryIndex as number) >= 0;
}

function isItemTypeDescriptor(value: unknown): value is ItemTypeDescriptor {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    isStringRecord(value.displayNames) &&
    isStringArrayValue(value.requiredLocales) &&
    Array.isArray(value.fields) &&
    value.fields.every(isItemFieldSpec) &&
    Array.isArray(value.evidenceQueries) &&
    value.evidenceQueries.every(
      (query) => isRecord(query) && isStringArrayValue(query.symbols) && isStringArrayValue(query.terms),
    ) &&
    isStringArrayValue(value.requiredResourceRoles)
  );
}

function isItemFieldSpec(value: unknown): value is ItemFieldSpec {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.required === "boolean" &&
    isStringRecord(value.displayNames) &&
    isItemFieldValueSpec(value.value)
  );
}

function isItemFieldValueSpec(value: unknown): value is ItemFieldValueSpec {
  if (!isRecord(value) || typeof value.kind !== "string") return false;
  switch (value.kind) {
    case "text":
      return typeof value.multiline === "boolean" && Number.isInteger(value.minLength) && Number.isInteger(value.maxLength);
    case "integer":
      return Number.isInteger(value.min) && Number.isInteger(value.max);
    case "boolean":
      return true;
    case "choice":
      return Array.isArray(value.options) && value.options.every(
        (option) => isRecord(option) && typeof option.value === "string" && isStringRecord(option.displayNames),
      );
    case "string_list":
      return Number.isInteger(value.minItems) && Number.isInteger(value.maxItems) && Number.isInteger(value.itemMaxLength);
    default:
      return false;
  }
}

function isStoredItemDefinition(value: unknown): value is StoredItemDefinition {
  return isRecord(value) && isSha256(value.definitionHash) && isItemDefinition(value.definition);
}

function isItemDefinition(value: unknown): value is ItemDefinition {
  return (
    isRecord(value) &&
    value.schemaVersion === 1 &&
    typeof value.itemId === "string" &&
    typeof value.itemType === "string" &&
    isRecord(value.canonicalFields) &&
    Object.values(value.canonicalFields).every(isItemFieldValue) &&
    isStringArrayValue(value.behaviorIntent) &&
    isRecord(value.localizations) &&
    Object.values(value.localizations).every(isItemLocalization) &&
    isRecord(value.resourceBindings) &&
    Object.values(value.resourceBindings).every(isItemResourceBinding)
  );
}

function isItemFieldValue(value: unknown): value is ItemFieldValue {
  if (!isRecord(value) || typeof value.kind !== "string") return false;
  switch (value.kind) {
    case "text":
    case "choice":
      return typeof value.value === "string";
    case "integer":
      return Number.isInteger(value.value);
    case "boolean":
      return typeof value.value === "boolean";
    case "string_list":
      return isStringArrayValue(value.value);
    default:
      return false;
  }
}

function isItemLocalization(value: unknown): value is ItemLocalization {
  return (
    isRecord(value) &&
    typeof value.name === "string" &&
    typeof value.description === "string" &&
    (value.status === "confirmed" || value.status === "outdated") &&
    (value.translatedFrom === undefined || value.translatedFrom === null || typeof value.translatedFrom === "string")
  );
}

function isItemResourceBinding(value: unknown): value is ItemResourceBinding {
  return isRecord(value) && typeof value.resourceId === "string" && isSha256(value.selectedVersion);
}

function isSha256(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}

function isStringRecord(value: unknown): value is Record<string, string> {
  return isRecord(value) && Object.values(value).every((item) => typeof item === "string");
}

function isStringArrayValue(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}
