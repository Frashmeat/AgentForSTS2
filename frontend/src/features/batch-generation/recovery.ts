import { getApproval, type ApprovalRequest } from "../../shared/api/index.ts";
import type {
  ExecutionBundlePreview,
  PlanItemValidation,
  PlanReviewPayload,
} from "../../shared/types/workflow.ts";
import {
  createDefaultBatchItemState,
  createInitialBatchRuntimeState,
  updateBatchItemStateRecord,
  type BatchItemState,
  type BatchItemStatus,
   type BatchRuntimeState,
  type BatchStage,
  type ReviewStrictness,
} from "./state.ts";

export const BATCH_RUNTIME_SNAPSHOT_KEY = "ats_batch_runtime_v1";

const SNAPSHOT_VERSION = 1;
const RECOVERABLE_BATCH_STAGES: BatchStage[] = [
  "input",
  "planning",
  "review_items",
  "review_bundles",
  "executing",
  "done",
  "error",
];
const RECOVERABLE_ITEM_STATUSES: BatchItemStatus[] = [
  "pending",
  "img_generating",
  "awaiting_selection",
  "approval_pending",
  "code_generating",
  "done",
  "error",
];
const MAX_PROGRESS_LINES = 20;
const MAX_AGENT_LOG_LINES = 20;
const MAX_IMAGES = 4;

type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

interface BatchItemSnapshot {
  status: BatchItemStatus;
  currentStage: string | null;
  stageHistory: string[];
  progress: string[];
  images: string[];
  agentLog: string[];
  currentAgentModel?: string | null;
  error: string | null;
  errorTrace: string | null;
  currentPrompt: string;
  approvalSummary: string;
  approvalRequests: ApprovalRequest[];
}

interface BatchRuntimeSnapshot {
  version: number;
  stage: BatchStage;
  activeItemId: string | null;
  currentBatchStage: string | null;
  batchStageHistory: string[];
  batchResult: { success: number; error: number } | null;
  itemStates: Record<string, BatchItemSnapshot>;
  planReview?: PlanReviewPayload | null;
  reviewStrictness?: ReviewStrictness;
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

function asStringArray(value: unknown, limit?: number): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const strings = value.filter((item): item is string => typeof item === "string");
  return typeof limit === "number" ? strings.slice(-limit) : strings;
}

function asStage(value: unknown): BatchStage {
  if (value === "review_plan") {
    return "review_items";
  }
  return RECOVERABLE_BATCH_STAGES.includes(value as BatchStage)
    ? (value as BatchStage)
    : "input";
}

function asReviewStrictness(value: unknown): ReviewStrictness {
  return value === "efficient" || value === "strict" ? value : "balanced";
}

function asItemStatus(value: unknown): BatchItemStatus {
  return RECOVERABLE_ITEM_STATUSES.includes(value as BatchItemStatus)
    ? (value as BatchItemStatus)
    : "pending";
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

function normalizePlanValidationItem(value: unknown): PlanItemValidation | null {
  if (!isRecord(value) || typeof value.item_id !== "string" || typeof value.status !== "string") {
    return null;
  }

  const status = value.status;
  if (status !== "clear" && status !== "needs_user_input" && status !== "invalid") {
    return null;
  }

  const issues = Array.isArray(value.issues)
    ? value.issues
        .filter((issue): issue is { code: string; message: string; field?: string } =>
          isRecord(issue) && typeof issue.code === "string" && typeof issue.message === "string",
        )
        .map((issue) => ({
          code: issue.code,
          message: issue.message,
          field: typeof issue.field === "string" ? issue.field : undefined,
        }))
    : [];

  return {
    item_id: value.item_id,
    status,
    issues,
    missing_fields: asStringArray(value.missing_fields),
    clarification_questions: asStringArray(value.clarification_questions),
  };
}

function normalizeExecutionBundlePreview(value: unknown): ExecutionBundlePreview | null {
  if (!isRecord(value) || !Array.isArray(value.item_ids) || typeof value.status !== "string" || typeof value.reason !== "string") {
    return null;
  }

  const status = value.status;
  if (status !== "clear" && status !== "needs_confirmation" && status !== "split_recommended") {
    return null;
  }

  return {
    item_ids: asStringArray(value.item_ids),
    status,
    reason: value.reason,
    risk_codes: asStringArray(value.risk_codes),
  };
}

function normalizePlanReviewPayload(value: unknown): PlanReviewPayload | null {
  if (!isRecord(value) || !isRecord(value.validation) || !isRecord(value.execution_plan)) {
    return null;
  }

  const validationItems = Array.isArray(value.validation.items)
    ? value.validation.items
        .map((item) => normalizePlanValidationItem(item))
        .filter((item): item is PlanItemValidation => item !== null)
    : [];

  const dependencyGroups = Array.isArray(value.execution_plan.dependency_groups)
    ? value.execution_plan.dependency_groups
        .filter((group): group is { item_ids: string[] } => isRecord(group) && Array.isArray(group.item_ids))
        .map((group) => ({ item_ids: asStringArray(group.item_ids) }))
    : [];

  const executionBundles = Array.isArray(value.execution_plan.execution_bundles)
    ? value.execution_plan.execution_bundles
        .map((bundle) => normalizeExecutionBundlePreview(bundle))
        .filter((bundle): bundle is ExecutionBundlePreview => bundle !== null)
    : [];

  return {
    strictness: asReviewStrictness(value.strictness),
    validation: {
      strictness: asReviewStrictness(value.validation.strictness),
      items: validationItems,
    },
    execution_plan: {
      strictness: asReviewStrictness(value.execution_plan.strictness),
      dependency_groups: dependencyGroups,
      execution_bundles: executionBundles,
    },
  };
}

function normalizeBatchItemSnapshot(value: unknown): BatchItemState {
  if (!isRecord(value)) {
    return createDefaultBatchItemState();
  }

  return {
    ...createDefaultBatchItemState(),
    status: asItemStatus(value.status),
    currentStage: asNullableString(value.currentStage),
    stageHistory: asStringArray(value.stageHistory),
    progress: asStringArray(value.progress),
    images: asStringArray(value.images),
    agentLog: asStringArray(value.agentLog),
    agentLogEntries: asStringArray(value.agentLog).map((text) => ({ text })),
    currentAgentModel: asNullableString(value.currentAgentModel),
    error: asNullableString(value.error),
    errorTrace: asNullableString(value.errorTrace),
    currentPrompt: asString(value.currentPrompt),
    approvalSummary: asString(value.approvalSummary),
    approvalRequests: normalizeApprovalRequests(value.approvalRequests),
  };
}

function normalizeBatchResult(value: unknown): { success: number; error: number } | null {
  if (!isRecord(value) || typeof value.success !== "number" || typeof value.error !== "number") {
    return null;
  }
  return {
    success: value.success,
    error: value.error,
  };
}

export function serializeBatchRuntimeSnapshot(state: BatchRuntimeState): BatchRuntimeSnapshot {
  return {
    version: SNAPSHOT_VERSION,
    stage: state.stage,
    activeItemId: state.activeItemId,
    currentBatchStage: state.currentBatchStage,
    batchStageHistory: state.batchStageHistory,
    batchResult: state.batchResult,
    planReview: state.planReview,
    reviewStrictness: state.reviewStrictness,
    itemStates: Object.fromEntries(
      Object.entries(state.itemStates).map(([itemId, itemState]) => [
        itemId,
        {
          status: itemState.status,
          currentStage: itemState.currentStage,
          stageHistory: itemState.stageHistory,
          progress: itemState.progress.slice(-MAX_PROGRESS_LINES),
          images: itemState.images.slice(-MAX_IMAGES),
          agentLog: itemState.agentLog.slice(-MAX_AGENT_LOG_LINES),
          currentAgentModel: itemState.currentAgentModel,
          error: itemState.error,
          errorTrace: itemState.errorTrace,
          currentPrompt: itemState.currentPrompt,
          approvalSummary: itemState.approvalSummary,
          approvalRequests: itemState.approvalRequests,
        } satisfies BatchItemSnapshot,
      ]),
    ),
  };
}

export function restoreBatchRuntimeSnapshot(snapshot: unknown): BatchRuntimeState | null {
  if (!isRecord(snapshot) || snapshot.version !== SNAPSHOT_VERSION || !isRecord(snapshot.itemStates)) {
    return null;
  }

  const baseState = createInitialBatchRuntimeState();
  return {
    ...baseState,
    stage: asStage(snapshot.stage),
    activeItemId: typeof snapshot.activeItemId === "string" ? snapshot.activeItemId : null,
    currentBatchStage: asNullableString(snapshot.currentBatchStage),
    batchStageHistory: asStringArray(snapshot.batchStageHistory),
    batchResult: normalizeBatchResult(snapshot.batchResult),
    planReview: normalizePlanReviewPayload(snapshot.planReview),
    reviewStrictness: asReviewStrictness(snapshot.reviewStrictness),
    itemStates: Object.fromEntries(
      Object.entries(snapshot.itemStates).map(([itemId, itemState]) => [
        itemId,
        normalizeBatchItemSnapshot(itemState),
      ]),
    ),
  };
}

export function loadBatchRuntimeSnapshot(storage: StorageLike = localStorage): BatchRuntimeState | null {
  try {
    const raw = storage.getItem(BATCH_RUNTIME_SNAPSHOT_KEY);
    if (!raw) {
      return null;
    }
    return restoreBatchRuntimeSnapshot(JSON.parse(raw));
  } catch {
    return null;
  }
}

export function saveBatchRuntimeSnapshot(
  storage: StorageLike = localStorage,
  state: BatchRuntimeState,
): void {
  try {
    storage.setItem(BATCH_RUNTIME_SNAPSHOT_KEY, JSON.stringify(serializeBatchRuntimeSnapshot(state)));
  } catch {}
}

export function clearBatchRuntimeSnapshot(storage: StorageLike = localStorage): void {
  try {
    storage.removeItem(BATCH_RUNTIME_SNAPSHOT_KEY);
  } catch {}
}

export function createRetryableBatchItemState(previous?: Partial<BatchItemState>): BatchItemState {
  return {
    ...createDefaultBatchItemState(),
    currentPrompt: typeof previous?.currentPrompt === "string" ? previous.currentPrompt : "",
    status: "img_generating",
  };
}

export async function refreshRecoveredBatchApprovals(
  runtimeState: BatchRuntimeState,
  fetchApproval: (actionId: string) => Promise<ApprovalRequest> = getApproval,
): Promise<BatchRuntimeState> {
  let itemStates = runtimeState.itemStates;

  for (const [itemId, itemState] of Object.entries(runtimeState.itemStates)) {
    if (itemState.status !== "approval_pending" || itemState.approvalRequests.length === 0) {
      continue;
    }

    const approvalRequests = await Promise.all(
      itemState.approvalRequests.map(async (request) => {
        try {
          return await fetchApproval(request.action_id);
        } catch {
          return request;
        }
      }),
    );

    itemStates = updateBatchItemStateRecord(itemStates, itemId, {
      approvalRequests,
      status: "approval_pending",
    });
  }

  return itemStates === runtimeState.itemStates
    ? runtimeState
    : {
        ...runtimeState,
        itemStates,
      };
}
