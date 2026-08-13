import { invoke } from "@tauri-apps/api/core";

import { toActionableFailure } from "./actionableFailure";
import { buildFeatureSubmission } from "./featureSubmission";
import {
  isCompositionConfirmation,
  isCompositionDraft,
  isStoredItemDefinition,
} from "./itemContractGuards";
import { isExecutionGraphView } from "./executionGraphContract";
import type { ExecutionGraphView } from "./executionGraphContract";
export { isActionableFailure, toActionableFailure } from "./actionableFailure";
export type { ActionableFailure, RecoveryAction } from "./actionableFailure";
export { isExecutionGraphView } from "./executionGraphContract";
export type {
  ExecutionGraphStatus,
  ExecutionGraphView,
} from "./executionGraphContract";

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
export type CancellationReason = "user" | "pause" | "project_close" | "project_switch" | "app_shutdown";
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

export async function listExecutionGraphs(): Promise<ExecutionGraphView[]> {
  const values = await invokeCommand<unknown>("list_execution_graphs");
  if (!Array.isArray(values) || !values.every(isExecutionGraphView)) {
    throw toActionableFailure(undefined);
  }
  return values;
}

export async function getExecutionGraph(executionGraphId: string): Promise<ExecutionGraphView> {
  const value = await invokeCommand<unknown>("get_execution_graph", { executionGraphId });
  if (!isExecutionGraphView(value)) throw toActionableFailure(undefined);
  return value;
}

export function pauseExecutionGraph(executionGraphId: string): Promise<boolean> {
  return invokeCommand<boolean>("pause_execution_graph", { executionGraphId });
}

export function cancelExecutionGraph(executionGraphId: string): Promise<boolean> {
  return invokeCommand<boolean>("cancel_execution_graph", { executionGraphId });
}

export function resumeExecutionGraph(
  executionGraphId: string,
  expectedRevision: number,
): Promise<string> {
  return invokeCommand<string>("resume_execution_graph", {
    executionGraphId,
    expectedRevision,
  });
}

export interface AdjustCompositionItemRequest {
  executionGraphId: string;
  expectedRevision: number;
  itemId: string;
  expectedDefinitionHash: string;
  instruction: string;
}

export function adjustCompositionItem(request: AdjustCompositionItemRequest): Promise<string> {
  return invokeCommand<string>("adjust_composition_item", { request });
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

export interface CompositionPlanRequest extends Record<string, unknown> {
  draftId: string;
  compositionId: string;
  concept: string;
  source: ItemCompositionSource;
  parameters: Record<string, number>;
}

export interface CompositionRetryNodeRequest extends Record<string, unknown> {
  draftId: string;
  expectedRevision: number;
  itemId: string;
  instructions: string;
}

export interface SingleGenerateRequest extends Record<string, unknown> {
  artifactId: string;
  modId: string;
  plan: PlanItem;
  definition: StoredItemDefinition;
}

export interface BatchDefinitionItem extends Record<string, unknown> {
  artifactId: string;
  definition: StoredItemDefinition;
}

export interface BatchGenerateRequest extends Record<string, unknown> {
  modId: string;
  items: BatchDefinitionItem[];
  failFast: boolean;
}

export interface SingleGenerateResult extends Record<string, unknown> {
  publication: "published" | "composition_staged";
  artifactManifestRef?: string | null;
  manifestSha256?: string | null;
  generatedFileCount: number;
  validationPrimitive: string;
  acceptanceNotes: string[];
}

export interface BatchItemResult extends Record<string, unknown> {
  itemId: string;
  definitionHash: string;
  planRunId: string;
  generationRunId?: string | null;
  status: RunStatus;
  plan?: PlanItem | null;
  result?: SingleGenerateResult | null;
  failureCode?: string | null;
}

export interface BatchGenerateResult extends Record<string, unknown> {
  total: number;
  processed: number;
  succeeded: number;
  failed: number;
  items: BatchItemResult[];
}

export interface ProjectPackageRequest extends Record<string, unknown> {
  artifactId: string;
  modId: string;
  sourceRelativeRoot: string;
  outputRelativePath: string;
  compressionLevel?: number | null;
}

export interface ProjectPackageResult extends Record<string, unknown> {
  publication: "published" | "composition_staged";
  artifactManifestRef?: string | null;
  manifestSha256?: string | null;
  outputRelativePath: string;
  report: {
    fileCount: number;
    uncompressedBytes: number;
    packageBytes: number;
  };
}

export interface CompositionGenerateRequest extends Record<string, unknown> {
  artifactId: string;
  modId: string;
  root: StoredItemDefinition;
  draft?: { draftId: string; revision: number } | null;
  package: ProjectPackageRequest;
  repairPolicy:
    | { kind: "until_passed" }
    | { kind: "max_rounds"; maxRounds: number };
  adjustment?: {
    itemId: string;
    expectedDefinitionHash: string;
    instruction: string;
    instructionSha256: string;
    createdAt: string;
  } | null;
  execution?:
    | { kind: "start"; executionGraphId: string }
    | {
        kind: "resume";
        executionGraphId: string;
        expectedRevision: number;
        previousRunId: string;
      }
    | null;
}

export interface ComplexGenerateRequest extends Record<string, unknown> {
  batch: BatchGenerateRequest;
  package: ProjectPackageRequest;
}

export interface ComplexGenerateResult extends Record<string, unknown> {
  batchRunId: string;
  batch: BatchGenerateResult;
  buildRunId?: string | null;
  build?: Record<string, unknown> | null;
  packageRunId?: string | null;
  package?: ProjectPackageResult | null;
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
    | { kind: "ai_generated"; prompt: string; model?: string | null };
}

export interface ProjectBuildRequest extends Record<string, unknown> {
  outputRelativeRoot?: string | null;
}

export function submitModPlan(request: ModPlanRequest): Promise<string> {
  return submit("mod.plan", "feature.mod-plan-request", request);
}

export function submitCompositionPlan(request: CompositionPlanRequest): Promise<string> {
  return submit("composition.plan", "feature.composition-plan-request", request);
}

export function submitCompositionRetryNode(request: CompositionRetryNodeRequest): Promise<string> {
  return submit("composition.retry-node", "feature.composition-retry-node-request", request);
}

export function submitCompositionGenerate(request: CompositionGenerateRequest): Promise<string> {
  return submit("composition.generate", "feature.composition-generate-request", request, 5);
}

export function submitSingleGenerate(request: SingleGenerateRequest): Promise<string> {
  return submit("mod.generate.single", "feature.mod-generate-single-request", request, 3);
}

export function submitBatchGenerate(request: BatchGenerateRequest): Promise<string> {
  return submit("mod.generate.batch", "feature.mod-generate-batch-request", request, 4);
}

export function submitComplexGenerate(request: ComplexGenerateRequest): Promise<string> {
  return submit("mod.generate.complex", "feature.mod-generate-complex-request", request, 3);
}

export function submitLogAnalyze(request: LogAnalyzeRequest): Promise<string> {
  return submit("log.analyze", "feature.log-analyze-request", request);
}

export function submitResourcePrepare(
  request: ResourcePrepareRequest,
  sourcePath?: string,
): Promise<string> {
  return submit("resource.prepare", "feature.resource-prepare-request", request, 2, sourcePath);
}

export function submitProjectBuild(request: ProjectBuildRequest = {}): Promise<string> {
  return submit("project.build", "feature.project-build-request", request, 2);
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

export interface LocalizationFieldSpec {
  id: string;
  displayNames: Record<string, string>;
  required: boolean;
  multiline: boolean;
  minLength: number;
  maxLength: number;
}

export interface ItemReferenceSlotSpec {
  id: string;
  displayNames: Record<string, string>;
  kind: "identity" | "pinned";
  allowedItemTypes: string[];
  minItems: number;
  maxItems: number;
  minQuantity: number;
  maxQuantity: number;
}

export interface ItemResourceProfileSpec {
  id: string;
  displayNames: Record<string, string>;
  requiredResourceRoles: string[];
}

export interface ItemTypeDescriptor {
  id: string;
  displayNames: Record<string, string>;
  requiredLocales: string[];
  fields: ItemFieldSpec[];
  localizationFields: LocalizationFieldSpec[];
  referenceSlots: ItemReferenceSlotSpec[];
  resourceProfileField?: string | null;
  resourceProfiles: ItemResourceProfileSpec[];
  evidenceQueries: { symbols: string[]; terms: string[] }[];
}

export interface CompositionParameterSpec {
  id: string;
  displayNames: Record<string, string>;
  min: number;
  max: number;
  nodeWeight: number;
}

export interface CompositionProfileSpec {
  id: string;
  displayNames: Record<string, string>;
  values: Record<string, number>;
}

export interface CompositionProfileSet {
  id: string;
  displayNames: Record<string, string>;
  rootItemType: string;
  defaultProfile: string;
  customBaseProfile: string;
  maxNodes: number;
  baseNodeCount: number;
  parameters: CompositionParameterSpec[];
  profiles: CompositionProfileSpec[];
  constraints: { kind: "less_or_equal"; left: string; right: string }[];
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
  compositionProfiles: CompositionProfileSet[];
}

export type ItemFieldValue =
  | { kind: "text"; value: string }
  | { kind: "integer"; value: number }
  | { kind: "boolean"; value: boolean }
  | { kind: "choice"; value: string }
  | { kind: "string_list"; value: string[] };

export interface ItemLocalization {
  fields: Record<string, string>;
  status: "confirmed" | "outdated";
  translatedFrom?: string | null;
}

export type ItemReferenceBinding =
  | { kind: "identity"; itemId: string; expectedItemType: string }
  | { kind: "pinned"; itemId: string; definitionHash: string; quantity: number };

export type ItemCompositionSource =
  | { kind: "preset"; profileId: string }
  | { kind: "custom"; baseProfileId: string };

export interface ItemCompositionProfile {
  compositionId: string;
  source: ItemCompositionSource;
  parameters: Record<string, number>;
}

export interface ItemResourceBinding {
  resourceId: string;
  selectedVersion: string;
}

export interface ItemDefinition {
  schemaVersion: 2;
  itemId: string;
  itemType: string;
  canonicalFields: Record<string, ItemFieldValue>;
  behaviorIntent: string[];
  localizations: Record<string, ItemLocalization>;
  resourceBindings: Record<string, ItemResourceBinding>;
  referenceBindings: Record<string, ItemReferenceBinding[]>;
  compositionProfile?: ItemCompositionProfile | null;
}

export interface StoredItemDefinition {
  definitionHash: string;
  definition: ItemDefinition;
}

export interface CompositionDraftNode {
  definition: ItemDefinition;
  expectedCurrentDefinitionHash?: string | null;
}

export interface CompositionDraft {
  schemaVersion: 2;
  draftId: string;
  revision: number;
  gamePackId: string;
  gamePackSha256: string;
  rootItemId: string;
  profile: ItemCompositionProfile;
  nodes: Record<string, CompositionDraftNode>;
  sourceExecutionGraphId?: string | null;
  validatedContentDigest?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CompositionConfirmation {
  draft: { draftId: string; revision: number };
  definitions: StoredItemDefinition[];
  confirmationDigest: string;
}

export type ResourceOrigin =
  | { kind: "user_upload" }
  | { kind: "ai_generated"; provider: string; model: string; request_sha256: string }
  | {
      kind: "pack_default";
      game_pack_id: string;
      game_pack_sha256: string;
      contribution_slot: string;
    };

export type ResourceVersionProvenance =
  | { kind: "original" }
  | {
      kind: "derived";
      sourceRole: string;
      sourceVersion: string;
      transform: string;
      transformVersion: number;
      parametersSha256: string;
      gamePackId: string;
      gamePackSha256: string;
    };

export interface ResourceBlob {
  relativePath: string;
  mediaType: string;
  byteLength: number;
  sha256: string;
  width: number;
  height: number;
  hasAlpha: boolean;
}

export interface ResourceVersion {
  id: string;
  parentVersion?: string | null;
  blob: ResourceBlob;
  provenance: ResourceVersionProvenance;
}

export interface ResourceAsset {
  schemaVersion: 2;
  resourceId: string;
  logicalRole: string;
  origin: ResourceOrigin;
  originalVersion: string;
  selectedVersion?: string | null;
  versions: ResourceVersion[];
}

export type ResourceRoleSource =
  | { kind: "master" }
  | { kind: "derived"; sourceRole: string };

export interface ResourceRoleDescriptor {
  id: string;
  mediaTypes: string[];
  width: number;
  height: number;
  requireAlpha: boolean;
  packDefaultAvailable: boolean;
  targetPath?: string | null;
  source: ResourceRoleSource;
}

export interface ResourceCatalog {
  gamePackId: string;
  gamePackSha256: string;
  roles: ResourceRoleDescriptor[];
}

export interface ResourcePreview {
  resourceId: string;
  logicalRole: string;
  version: string;
  mediaType: string;
  width: number;
  height: number;
  hasAlpha: boolean;
  dataUrl: string;
}

export interface ResourceCandidateResult {
  resourceId: string;
  logicalRole: string;
  origin: ResourceOrigin;
  candidateVersion: string;
  selectedVersion?: string | null;
  mediaType: string;
  width: number;
  height: number;
  hasAlpha: boolean;
}

export interface ResourcePrepareResult {
  candidates: ResourceCandidateResult[];
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

export async function listCompositionDrafts(): Promise<CompositionDraft[]> {
  const value = await invokeCommand<unknown>("list_composition_drafts");
  if (!Array.isArray(value) || !value.every(isCompositionDraft)) {
    throw toActionableFailure(undefined);
  }
  return value;
}

export async function getCompositionDraft(draftId: string): Promise<CompositionDraft> {
  const value = await invokeCommand<unknown>("get_composition_draft", { draftId });
  if (!isCompositionDraft(value)) throw toActionableFailure(undefined);
  return value;
}

export async function updateCompositionDraft(
  draftId: string,
  expectedRevision: number,
  nodes: Record<string, CompositionDraftNode>,
): Promise<CompositionDraft> {
  const value = await invokeCommand<unknown>("update_composition_draft", {
    draftId,
    expectedRevision,
    nodes,
  });
  if (!isCompositionDraft(value)) throw toActionableFailure(undefined);
  return value;
}

export function deleteCompositionDraft(
  draftId: string,
  expectedRevision: number,
): Promise<void> {
  return invokeCommand<void>("delete_composition_draft", { draftId, expectedRevision });
}

export async function confirmCompositionDraft(
  draftId: string,
  expectedRevision: number,
  selectedItemIds: string[],
): Promise<CompositionConfirmation> {
  const value = await invokeCommand<unknown>("confirm_composition_draft", {
    draftId,
    expectedRevision,
    selectedItemIds,
  });
  if (!isCompositionConfirmation(value)) throw toActionableFailure(undefined);
  return value;
}

export async function getResourceCatalog(): Promise<ResourceCatalog> {
  const value = await invokeCommand<unknown>("get_resource_catalog");
  if (!isResourceCatalog(value)) throw toActionableFailure(undefined);
  return value;
}

export async function listResourceAssets(): Promise<ResourceAsset[]> {
  const value = await invokeCommand<unknown>("list_resource_assets");
  if (!Array.isArray(value) || !value.every(isResourceAsset)) {
    throw toActionableFailure(undefined);
  }
  return value;
}

export async function getResourcePreview(
  resourceId: string,
  version: string,
): Promise<ResourcePreview> {
  const value = await invokeCommand<unknown>("get_resource_preview", { resourceId, version });
  if (!isResourcePreview(value)) throw toActionableFailure(undefined);
  return value;
}

export async function selectResource(
  resourceId: string,
  version: string,
): Promise<ResourcePrepareResult> {
  const value = await invokeCommand<unknown>("select_resource", { resourceId, version });
  if (!isResourcePrepareResult(value)) throw toActionableFailure(undefined);
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
  return isRecord(value) &&
    typeof value.code === "string" &&
    typeof value.stage === "string" &&
    (value.details === undefined || value.details === null || isVersionedPayload(value.details));
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
    value.itemTypes.every(isItemTypeCapability) &&
    Array.isArray(value.compositionProfiles) &&
    value.compositionProfiles.every(isCompositionProfileSet)
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
    Array.isArray(value.localizationFields) &&
    value.localizationFields.every(isLocalizationFieldSpec) &&
    Array.isArray(value.referenceSlots) &&
    value.referenceSlots.every(isItemReferenceSlotSpec) &&
    (value.resourceProfileField === undefined || value.resourceProfileField === null || typeof value.resourceProfileField === "string") &&
    Array.isArray(value.resourceProfiles) &&
    value.resourceProfiles.every(isItemResourceProfileSpec) &&
    Array.isArray(value.evidenceQueries) &&
    value.evidenceQueries.every(
      (query) => isRecord(query) && isStringArrayValue(query.symbols) && isStringArrayValue(query.terms),
    )
  );
}

function isLocalizationFieldSpec(value: unknown): value is LocalizationFieldSpec {
  return isRecord(value) && typeof value.id === "string" && isStringRecord(value.displayNames) &&
    typeof value.required === "boolean" && typeof value.multiline === "boolean" &&
    Number.isInteger(value.minLength) && Number.isInteger(value.maxLength);
}

function isItemReferenceSlotSpec(value: unknown): value is ItemReferenceSlotSpec {
  return isRecord(value) && typeof value.id === "string" && isStringRecord(value.displayNames) &&
    (value.kind === "identity" || value.kind === "pinned") && isStringArrayValue(value.allowedItemTypes) &&
    Number.isInteger(value.minItems) && Number.isInteger(value.maxItems) &&
    Number.isInteger(value.minQuantity) && Number.isInteger(value.maxQuantity);
}

function isItemResourceProfileSpec(value: unknown): value is ItemResourceProfileSpec {
  return isRecord(value) && typeof value.id === "string" && isStringRecord(value.displayNames) &&
    isStringArrayValue(value.requiredResourceRoles);
}

function isCompositionProfileSet(value: unknown): value is CompositionProfileSet {
  return isRecord(value) && typeof value.id === "string" && isStringRecord(value.displayNames) &&
    typeof value.rootItemType === "string" && typeof value.defaultProfile === "string" &&
    typeof value.customBaseProfile === "string" && isPositiveInteger(value.maxNodes) &&
    isPositiveInteger(value.baseNodeCount) && Array.isArray(value.parameters) &&
    value.parameters.every(isCompositionParameterSpec) && Array.isArray(value.profiles) &&
    value.profiles.every(isCompositionProfileSpec) && Array.isArray(value.constraints) &&
    value.constraints.every((constraint) => isRecord(constraint) && constraint.kind === "less_or_equal" &&
      typeof constraint.left === "string" && typeof constraint.right === "string");
}

function isCompositionParameterSpec(value: unknown): value is CompositionParameterSpec {
  return isRecord(value) && typeof value.id === "string" && isStringRecord(value.displayNames) &&
    Number.isInteger(value.min) && Number.isInteger(value.max) && Number.isInteger(value.nodeWeight);
}

function isCompositionProfileSpec(value: unknown): value is CompositionProfileSpec {
  return isRecord(value) && typeof value.id === "string" && isStringRecord(value.displayNames) &&
    isRecord(value.values) && Object.values(value.values).every(Number.isInteger);
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

function isResourceCatalog(value: unknown): value is ResourceCatalog {
  return (
    isRecord(value) &&
    typeof value.gamePackId === "string" &&
    isSha256(value.gamePackSha256) &&
    Array.isArray(value.roles) &&
    value.roles.every(isResourceRoleDescriptor)
  );
}

function isResourceRoleDescriptor(value: unknown): value is ResourceRoleDescriptor {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    isStringArrayValue(value.mediaTypes) &&
    isPositiveInteger(value.width) &&
    isPositiveInteger(value.height) &&
    typeof value.requireAlpha === "boolean" &&
    typeof value.packDefaultAvailable === "boolean" &&
    (value.targetPath === undefined || value.targetPath === null || typeof value.targetPath === "string") &&
    isRecord(value.source) &&
    (value.source.kind === "master" ||
      (value.source.kind === "derived" && typeof value.source.sourceRole === "string"))
  );
}

function isResourceAsset(value: unknown): value is ResourceAsset {
  return (
    isRecord(value) &&
    value.schemaVersion === 2 &&
    typeof value.resourceId === "string" &&
    typeof value.logicalRole === "string" &&
    isResourceOrigin(value.origin) &&
    isSha256(value.originalVersion) &&
    (value.selectedVersion === undefined || value.selectedVersion === null || isSha256(value.selectedVersion)) &&
    Array.isArray(value.versions) &&
    value.versions.length > 0 &&
    value.versions.every(isResourceVersion)
  );
}

function isResourceOrigin(value: unknown): value is ResourceOrigin {
  if (!isRecord(value)) return false;
  if (value.kind === "user_upload") return true;
  if (value.kind === "ai_generated") {
    return typeof value.provider === "string" && typeof value.model === "string" && isSha256(value.request_sha256);
  }
  return (
    value.kind === "pack_default" &&
    typeof value.game_pack_id === "string" &&
    isSha256(value.game_pack_sha256) &&
    typeof value.contribution_slot === "string"
  );
}

function isResourceVersion(value: unknown): value is ResourceVersion {
  return (
    isRecord(value) &&
    isSha256(value.id) &&
    (value.parentVersion === undefined || value.parentVersion === null || isSha256(value.parentVersion)) &&
    isResourceBlob(value.blob) &&
    isResourceVersionProvenance(value.provenance)
  );
}

function isResourceBlob(value: unknown): value is ResourceBlob {
  return (
    isRecord(value) &&
    typeof value.relativePath === "string" &&
    typeof value.mediaType === "string" &&
    isPositiveInteger(value.byteLength) &&
    isSha256(value.sha256) &&
    isPositiveInteger(value.width) &&
    isPositiveInteger(value.height) &&
    typeof value.hasAlpha === "boolean"
  );
}

function isResourceVersionProvenance(value: unknown): value is ResourceVersionProvenance {
  if (!isRecord(value)) return false;
  if (value.kind === "original") return true;
  return (
    value.kind === "derived" &&
    typeof value.sourceRole === "string" &&
    isSha256(value.sourceVersion) &&
    typeof value.transform === "string" &&
    isPositiveInteger(value.transformVersion) &&
    isSha256(value.parametersSha256) &&
    typeof value.gamePackId === "string" &&
    isSha256(value.gamePackSha256)
  );
}

function isResourcePreview(value: unknown): value is ResourcePreview {
  return (
    isRecord(value) &&
    typeof value.resourceId === "string" &&
    typeof value.logicalRole === "string" &&
    isSha256(value.version) &&
    value.mediaType === "image/png" &&
    isPositiveInteger(value.width) &&
    isPositiveInteger(value.height) &&
    typeof value.hasAlpha === "boolean" &&
    typeof value.dataUrl === "string" &&
    value.dataUrl.startsWith("data:image/png;base64,")
  );
}

function isResourcePrepareResult(value: unknown): value is ResourcePrepareResult {
  return isRecord(value) && Array.isArray(value.candidates) && value.candidates.every(isResourceCandidateResult);
}

function isResourceCandidateResult(value: unknown): value is ResourceCandidateResult {
  return (
    isRecord(value) &&
    typeof value.resourceId === "string" &&
    typeof value.logicalRole === "string" &&
    isResourceOrigin(value.origin) &&
    isSha256(value.candidateVersion) &&
    (value.selectedVersion === undefined || value.selectedVersion === null || isSha256(value.selectedVersion)) &&
    typeof value.mediaType === "string" &&
    isPositiveInteger(value.width) &&
    isPositiveInteger(value.height) &&
    typeof value.hasAlpha === "boolean"
  );
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
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
