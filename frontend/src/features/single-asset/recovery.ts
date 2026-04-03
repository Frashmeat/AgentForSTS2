import { getApproval, type ApprovalRequest } from "../../shared/api/index.ts";
import type { AssetType, Stage } from "./model.ts";
import {
  createInitialSingleAssetWorkflowState,
  type SingleAssetWorkflowState,
} from "./state.ts";

export const SINGLE_ASSET_SNAPSHOT_KEY = "ats_single_asset_v1";

const SNAPSHOT_VERSION = 1;
const MAX_IMAGES = 4;
const MAX_GEN_LOG_LINES = 20;
const MAX_AGENT_LOG_LINES = 20;

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

const STAGES: Stage[] = [
  "input",
  "confirm_prompt",
  "generating_image",
  "pick_image",
  "agent_running",
  "approval_pending",
  "done",
  "error",
];
const ASSET_TYPES: AssetType[] = ["card", "card_fullscreen", "relic", "power", "character"];

export interface SingleAssetRecoveryState {
  assetType: AssetType;
  assetName: string;
  description: string;
  projectRoot: string;
  imageMode: "ai" | "upload";
  autoMode: boolean;
  uploadedImageB64: string;
  uploadedImageName: string;
  uploadedImagePreview: string | null;
  workflowState: SingleAssetWorkflowState;
}

interface SingleAssetWorkflowSnapshot {
  stage: Stage;
  images: string[];
  pendingSlots: number;
  promptPreview: string;
  negativePrompt: string;
  promptFallbackWarn: string | null;
  currentPrompt: string;
  showMorePrompt: boolean;
  genLog: string[];
  agentLog: string[];
  flowStageCurrent: string | null;
  flowStageHistory: string[];
  agentStageCurrent: string | null;
  agentStageHistory: string[];
  approvalSummary: string;
  approvalRequests: ApprovalRequest[];
  errorMessage: string | null;
  errorTraceback: string | null;
}

interface SingleAssetSnapshot {
  version: number;
  assetType: AssetType;
  assetName: string;
  description: string;
  projectRoot: string;
  imageMode: "ai" | "upload";
  autoMode: boolean;
  uploadedImageB64: string;
  uploadedImageName: string;
  uploadedImagePreview: string | null;
  workflowState: SingleAssetWorkflowSnapshot;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function asNullableString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function asBoolean(value: unknown, fallback = false): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function asStringArray(value: unknown, limit?: number): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const strings = value.filter((item): item is string => typeof item === "string");
  return typeof limit === "number" ? strings.slice(-limit) : strings;
}

function asStage(value: unknown): Stage {
  return STAGES.includes(value as Stage) ? (value as Stage) : "input";
}

function asAssetType(value: unknown): AssetType {
  return ASSET_TYPES.includes(value as AssetType) ? (value as AssetType) : "relic";
}

function normalizeApprovalRequest(value: unknown): ApprovalRequest | null {
  if (!isRecord(value)) {
    return null;
  }
  if (
    typeof value.action_id !== "string" ||
    typeof value.kind !== "string" ||
    typeof value.title !== "string" ||
    typeof value.reason !== "string" ||
    (value.risk_level !== "low" && value.risk_level !== "medium" && value.risk_level !== "high") ||
    typeof value.requires_approval !== "boolean" ||
    typeof value.status !== "string"
  ) {
    return null;
  }

  return {
    action_id: value.action_id,
    kind: value.kind,
    title: value.title,
    reason: value.reason,
    risk_level: value.risk_level,
    requires_approval: value.requires_approval,
    status: value.status,
    payload: isRecord(value.payload) ? value.payload : {},
    source_backend: typeof value.source_backend === "string" ? value.source_backend : undefined,
    source_workflow: typeof value.source_workflow === "string" ? value.source_workflow : undefined,
    created_at: typeof value.created_at === "string" ? value.created_at : undefined,
    result: "result" in value ? value.result : undefined,
    error: "error" in value ? value.error : undefined,
  };
}

function normalizeApprovalRequests(value: unknown): ApprovalRequest[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => normalizeApprovalRequest(item))
    .filter((item): item is ApprovalRequest => item !== null);
}

function normalizeWorkflowSnapshot(value: unknown): SingleAssetWorkflowState {
  if (!isRecord(value)) {
    return createInitialSingleAssetWorkflowState();
  }

  return {
    ...createInitialSingleAssetWorkflowState(),
    stage: asStage(value.stage),
    images: asStringArray(value.images, MAX_IMAGES),
    pendingSlots: typeof value.pendingSlots === "number" ? value.pendingSlots : 0,
    promptPreview: asString(value.promptPreview),
    negativePrompt: asString(value.negativePrompt),
    promptFallbackWarn: asNullableString(value.promptFallbackWarn),
    currentPrompt: asString(value.currentPrompt),
    showMorePrompt: asBoolean(value.showMorePrompt),
    genLog: asStringArray(value.genLog, MAX_GEN_LOG_LINES),
    agentLog: asStringArray(value.agentLog, MAX_AGENT_LOG_LINES),
    flowStageCurrent: asNullableString(value.flowStageCurrent),
    flowStageHistory: asStringArray(value.flowStageHistory),
    agentStageCurrent: asNullableString(value.agentStageCurrent),
    agentStageHistory: asStringArray(value.agentStageHistory),
    approvalSummary: asString(value.approvalSummary),
    approvalRequests: normalizeApprovalRequests(value.approvalRequests),
    approvalBusyActionId: null,
    errorMessage: asNullableString(value.errorMessage ?? value.errorMsg),
    errorTraceback: asNullableString(value.errorTraceback ?? value.errorTrace),
  };
}

export function serializeSingleAssetSnapshot(state: SingleAssetRecoveryState): SingleAssetSnapshot {
  return {
    version: SNAPSHOT_VERSION,
    assetType: state.assetType,
    assetName: state.assetName,
    description: state.description,
    projectRoot: state.projectRoot,
    imageMode: state.imageMode,
    autoMode: state.autoMode,
    uploadedImageB64: state.uploadedImageB64,
    uploadedImageName: state.uploadedImageName,
    uploadedImagePreview: state.uploadedImagePreview,
    workflowState: {
      stage: state.workflowState.stage,
      images: state.workflowState.images.slice(-MAX_IMAGES),
      pendingSlots: state.workflowState.pendingSlots,
      promptPreview: state.workflowState.promptPreview,
      negativePrompt: state.workflowState.negativePrompt,
      promptFallbackWarn: state.workflowState.promptFallbackWarn,
      currentPrompt: state.workflowState.currentPrompt,
      showMorePrompt: state.workflowState.showMorePrompt,
      genLog: state.workflowState.genLog.slice(-MAX_GEN_LOG_LINES),
      agentLog: state.workflowState.agentLog.slice(-MAX_AGENT_LOG_LINES),
      flowStageCurrent: state.workflowState.flowStageCurrent,
      flowStageHistory: state.workflowState.flowStageHistory,
      agentStageCurrent: state.workflowState.agentStageCurrent,
      agentStageHistory: state.workflowState.agentStageHistory,
      approvalSummary: state.workflowState.approvalSummary,
      approvalRequests: state.workflowState.approvalRequests,
      errorMessage: state.workflowState.errorMessage,
      errorTraceback: state.workflowState.errorTraceback,
    },
  };
}

export function restoreSingleAssetSnapshot(snapshot: unknown): SingleAssetRecoveryState | null {
  if (!isRecord(snapshot) || snapshot.version !== SNAPSHOT_VERSION || !isRecord(snapshot.workflowState)) {
    return null;
  }

  return {
    assetType: asAssetType(snapshot.assetType),
    assetName: asString(snapshot.assetName),
    description: asString(snapshot.description),
    projectRoot: asString(snapshot.projectRoot),
    imageMode: snapshot.imageMode === "upload" ? "upload" : "ai",
    autoMode: asBoolean(snapshot.autoMode),
    uploadedImageB64: asString(snapshot.uploadedImageB64),
    uploadedImageName: asString(snapshot.uploadedImageName),
    uploadedImagePreview: asNullableString(snapshot.uploadedImagePreview),
    workflowState: normalizeWorkflowSnapshot(snapshot.workflowState),
  };
}

export function loadSingleAssetSnapshot(storage: StorageLike = localStorage): SingleAssetRecoveryState | null {
  try {
    const raw = storage.getItem(SINGLE_ASSET_SNAPSHOT_KEY);
    if (!raw) {
      return null;
    }
    return restoreSingleAssetSnapshot(JSON.parse(raw));
  } catch {
    return null;
  }
}

export function saveSingleAssetSnapshot(
  storage: StorageLike = localStorage,
  state: SingleAssetRecoveryState,
): void {
  try {
    storage.setItem(SINGLE_ASSET_SNAPSHOT_KEY, JSON.stringify(serializeSingleAssetSnapshot(state)));
  } catch {}
}

export function clearSingleAssetSnapshot(storage: StorageLike = localStorage): void {
  try {
    storage.removeItem(SINGLE_ASSET_SNAPSHOT_KEY);
  } catch {}
}

export async function refreshRecoveredSingleAssetApprovals(
  state: SingleAssetRecoveryState,
  fetchApproval: (actionId: string) => Promise<ApprovalRequest> = getApproval,
): Promise<SingleAssetRecoveryState> {
  if (
    state.workflowState.stage !== "approval_pending" ||
    state.workflowState.approvalRequests.length === 0
  ) {
    return state;
  }

  const approvalRequests = await Promise.all(
    state.workflowState.approvalRequests.map(async (request) => {
      try {
        return await fetchApproval(request.action_id);
      } catch {
        return request;
      }
    }),
  );

  return {
    ...state,
    workflowState: {
      ...state.workflowState,
      approvalRequests,
    },
  };
}

export function hasSingleAssetRecoveryContext(state: SingleAssetRecoveryState): boolean {
  return Boolean(
    state.assetName.trim() ||
    state.description.trim() ||
    state.projectRoot.trim() ||
    state.uploadedImageB64 ||
    state.workflowState.stage !== "input" ||
    state.workflowState.images.length > 0 ||
    state.workflowState.promptPreview.trim() ||
    state.workflowState.approvalRequests.length > 0 ||
    state.workflowState.errorMessage ||
    state.workflowState.genLog.length > 0 ||
    state.workflowState.agentLog.length > 0,
  );
}
