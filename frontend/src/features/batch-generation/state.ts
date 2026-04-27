import type { ApprovalRequest } from "../../shared/api/index.ts";
import type { ExecutionBundlePreview, PlanReviewPayload, WorkflowLogChannel } from "../../shared/types/workflow.ts";
import {
  appendWorkflowLogEntry,
  resolveNextWorkflowModel,
  type WorkflowLogEntry,
} from "../../shared/workflowLog.ts";

export type BatchItemStatus =
  | "pending"
  | "img_generating"
  | "awaiting_selection"
  | "approval_pending"
  | "code_generating"
  | "cancelled"
  | "done"
  | "error";

export interface BatchItemState {
  status: BatchItemStatus;
  currentStage: string | null;
  stageHistory: string[];
  progress: string[];
  images: string[];
  agentLog: string[];
  agentLogEntries: WorkflowLogEntry[];
  currentAgentModel: string | null;
  error: string | null;
  errorTrace: string | null;
  currentPrompt: string;
  showMorePrompt: boolean;
  approvalSummary: string;
  approvalRequests: ApprovalRequest[];
}

export type BatchItemStateRecord = Record<string, BatchItemState>;

export type ReviewStrictness = "efficient" | "balanced" | "strict";
export type BundleDecisionStatus = "unresolved" | "accepted" | "split_requested" | "needs_item_revision";
export type BundleDecisionRecord = Record<string, BundleDecisionStatus>;
export type BatchStage =
  | "input"
  | "planning"
  | "review_items"
  | "review_bundles"
  | "executing"
  | "cancelled"
  | "done"
  | "error";

export interface BatchRuntimeState {
  stage: BatchStage;
  itemStates: BatchItemStateRecord;
  activeItemId: string | null;
  batchLog: string[];
  currentBatchStage: string | null;
  batchStageHistory: string[];
  workflowErrorMessage: string | null;
  batchResult: { success: number; error: number } | null;
  approvalBusyActionId: string | null;
  planReview: PlanReviewPayload | null;
  reviewStrictness: ReviewStrictness;
  bundleDecisions: BundleDecisionRecord;
}

export type BatchRuntimeAction =
  | { type: "planning_started" }
  | { type: "stage_set"; stage: BatchStage }
  | { type: "plan_ready_received"; review?: PlanReviewPayload | null; decisions?: BundleDecisionRecord }
  | { type: "review_updated"; review: PlanReviewPayload | null; decisions: BundleDecisionRecord }
  | { type: "review_items_confirmed" }
  | { type: "review_bundles_confirmed" }
  | { type: "review_strictness_set"; strictness: ReviewStrictness }
  | { type: "bundle_decisions_set"; decisions: BundleDecisionRecord }
  | { type: "batch_log_appended"; message: string }
  | { type: "batch_stage_message"; message: string }
  | { type: "batch_started"; items: Array<{ id: string }> }
  | { type: "item_started"; itemId: string }
  | { type: "item_progress_received"; itemId: string; message: string }
  | { type: "item_image_ready"; itemId: string; image: string; index: number; prompt: string }
  | { type: "item_agent_stream"; itemId: string; chunk: string; source?: string; channel?: WorkflowLogChannel; model?: string }
  | { type: "item_approval_pending"; itemId: string; summary: string; requests: ApprovalRequest[] }
  | { type: "item_done"; itemId: string }
  | { type: "item_error"; itemId: string; message: string; traceback: string | null }
  | { type: "batch_done"; success: number; error: number }
  | { type: "workflow_cancelled"; message?: string }
  | { type: "workflow_failed"; message: string }
  | { type: "workflow_reset" }
  | { type: "active_item_set"; itemId: string | null }
  | { type: "approval_busy_set"; actionId: string | null }
  | { type: "approval_request_updated"; actionId: string; request: ApprovalRequest }
  | { type: "item_state_patched"; itemId: string; patch: Partial<BatchItemState> }
  | { type: "item_stage_message"; itemId: string; message: string };

export function createDefaultBatchItemState(): BatchItemState {
  return {
    status: "pending",
    currentStage: null,
    stageHistory: [],
    progress: [],
    images: [],
    agentLog: [],
    agentLogEntries: [],
    currentAgentModel: null,
    error: null,
    errorTrace: null,
    currentPrompt: "",
    showMorePrompt: false,
    approvalSummary: "",
    approvalRequests: [],
  };
}

export function createBatchItemStateRecord(items: Array<{ id: string }>): BatchItemStateRecord {
  return Object.fromEntries(items.map((item) => [item.id, createDefaultBatchItemState()]));
}

export function createInitialBatchRuntimeState(): BatchRuntimeState {
  return {
    stage: "input",
    itemStates: {},
    activeItemId: null,
    batchLog: [],
    currentBatchStage: null,
    batchStageHistory: [],
    workflowErrorMessage: null,
    batchResult: null,
    approvalBusyActionId: null,
    planReview: null,
    reviewStrictness: "balanced",
    bundleDecisions: {},
  };
}

export function canProceedFromItemReview(review: PlanReviewPayload | null | undefined): boolean {
  if (!review) {
    return true;
  }
  return review.validation.items.every((item) => item.status === "clear");
}

export function resolveExecutionBundleKey(bundle: ExecutionBundlePreview, index = 0): string {
  if (bundle.bundle_id && bundle.bundle_id.trim().length > 0) {
    return bundle.bundle_id;
  }
  if (bundle.item_ids.length > 0) {
    return `bundle:${bundle.item_ids.join("::")}`;
  }
  return `bundle:${index + 1}`;
}

export function reconcileBundleDecisionRecord(
  review: PlanReviewPayload | null | undefined,
  previous: BundleDecisionRecord = {},
): BundleDecisionRecord {
  if (!review) {
    return {};
  }

  const next: BundleDecisionRecord = {};
  review.execution_plan.execution_bundles.forEach((bundle, index) => {
    if (bundle.status === "clear") {
      return;
    }
    const key = resolveExecutionBundleKey(bundle, index);
    next[key] = previous[key] ?? "unresolved";
  });
  return next;
}

export function canProceedFromBundleReview(
  review: PlanReviewPayload | null | undefined,
  decisions: BundleDecisionRecord = {},
): boolean {
  if (!review) {
    return true;
  }
  return review.execution_plan.execution_bundles.every((bundle, index) => {
    if (bundle.status === "clear") {
      return true;
    }
    return decisions[resolveExecutionBundleKey(bundle, index)] === "accepted";
  });
}

export function summarizeBundleDecisionProgress(
  review: PlanReviewPayload | null | undefined,
  decisions: BundleDecisionRecord = {},
): {
  total: number;
  clear: number;
  accepted: number;
  splitRequested: number;
  needsItemRevision: number;
  unresolved: number;
  blocking: number;
} {
  if (!review) {
    return {
      total: 0,
      clear: 0,
      accepted: 0,
      splitRequested: 0,
      needsItemRevision: 0,
      unresolved: 0,
      blocking: 0,
    };
  }

  let clear = 0;
  let accepted = 0;
  let splitRequested = 0;
  let needsItemRevision = 0;
  let unresolved = 0;

  review.execution_plan.execution_bundles.forEach((bundle, index) => {
    if (bundle.status === "clear") {
      clear += 1;
      return;
    }
    const decision = decisions[resolveExecutionBundleKey(bundle, index)] ?? "unresolved";
    if (decision === "accepted") {
      accepted += 1;
    } else if (decision === "split_requested") {
      splitRequested += 1;
    } else if (decision === "needs_item_revision") {
      needsItemRevision += 1;
    } else {
      unresolved += 1;
    }
  });

  return {
    total: review.execution_plan.execution_bundles.length,
    clear,
    accepted,
    splitRequested,
    needsItemRevision,
    unresolved,
    blocking: splitRequested + needsItemRevision + unresolved,
  };
}

export function updateBatchItemStateRecord(
  record: BatchItemStateRecord,
  id: string,
  patch: Partial<BatchItemState>,
): BatchItemStateRecord {
  return {
    ...record,
    [id]: { ...(record[id] ?? createDefaultBatchItemState()), ...patch },
  };
}

export function appendBatchItemProgress(
  record: BatchItemStateRecord,
  id: string,
  message: string,
): BatchItemStateRecord {
  const current = record[id] ?? createDefaultBatchItemState();
  return {
    ...record,
    [id]: {
      ...current,
      progress: [...current.progress, message],
    },
  };
}

export function appendBatchItemAgentLog(
  record: BatchItemStateRecord,
  id: string,
  entry: WorkflowLogEntry,
): BatchItemStateRecord {
  const current = record[id] ?? createDefaultBatchItemState();
  return {
    ...record,
    [id]: {
      ...current,
      agentLog: [...current.agentLog, entry.text],
      agentLogEntries: appendWorkflowLogEntry(current.agentLogEntries, entry),
      currentAgentModel: resolveNextWorkflowModel(current.currentAgentModel, entry),
    },
  };
}

export function applyBatchItemStageMessage(
  record: BatchItemStateRecord,
  id: string,
  message: string,
): BatchItemStateRecord {
  const current = record[id] ?? createDefaultBatchItemState();
  return {
    ...record,
    [id]: {
      ...current,
      currentStage: message,
      stageHistory: current.stageHistory[current.stageHistory.length - 1] === message
        ? current.stageHistory
        : [...current.stageHistory, message],
    },
  };
}

export function applyBatchItemImage(
  record: BatchItemStateRecord,
  id: string,
  image: string,
  index: number,
  prompt: string,
): BatchItemStateRecord {
  const current = record[id] ?? createDefaultBatchItemState();
  const images = [...current.images];
  images[index] = image;
  return {
    ...record,
    [id]: {
      ...current,
      images,
      currentPrompt: prompt,
      status: "awaiting_selection",
    },
  };
}

function pushHistory(history: string[], message: string): string[] {
  return history[history.length - 1] === message ? history : [...history, message];
}

function needsAttention(status?: BatchItemStatus): boolean {
  return status === "awaiting_selection" || status === "approval_pending" || status === "img_generating" || status === "code_generating";
}

function pickNextActiveOnStart(currentActiveId: string | null, itemStates: BatchItemStateRecord, startedItemId: string): string {
  if (!currentActiveId) return startedItemId;

  const currentStatus = itemStates[currentActiveId]?.status;
  if (currentStatus === "awaiting_selection") return currentActiveId;
  if (currentStatus === "done" || currentStatus === "error") return startedItemId;
  return currentActiveId;
}

function pickNextActiveOnDone(currentActiveId: string | null, doneItemId: string, itemStates: BatchItemStateRecord): string | null {
  if (currentActiveId !== doneItemId) return currentActiveId;

  const next = Object.entries(itemStates).find(
    ([id, state]) => id !== doneItemId && needsAttention(state.status),
  );
  return next ? next[0] : currentActiveId;
}

export function batchWorkflowReducer(state: BatchRuntimeState, action: BatchRuntimeAction): BatchRuntimeState {
  switch (action.type) {
    case "planning_started":
      return {
        ...createInitialBatchRuntimeState(),
        stage: "planning",
      };
    case "stage_set":
      return {
        ...state,
        stage: action.stage,
      };
    case "plan_ready_received":
      return {
        ...state,
        stage: "review_items",
        planReview: action.review ?? null,
        reviewStrictness: action.review?.strictness === "efficient" || action.review?.strictness === "strict"
          ? action.review.strictness
          : "balanced",
        bundleDecisions: action.decisions ?? {},
      };
    case "review_updated":
      return {
        ...state,
        planReview: action.review,
        reviewStrictness: action.review?.strictness === "efficient" || action.review?.strictness === "strict"
          ? action.review.strictness
          : "balanced",
        bundleDecisions: action.decisions,
      };
    case "review_items_confirmed":
      return {
        ...state,
        stage: "review_bundles",
      };
    case "review_bundles_confirmed":
      return {
        ...state,
        stage: "executing",
      };
    case "review_strictness_set":
      return {
        ...state,
        reviewStrictness: action.strictness,
      };
    case "bundle_decisions_set":
      return {
        ...state,
        bundleDecisions: action.decisions,
      };
    case "batch_log_appended":
      return {
        ...state,
        batchLog: [...state.batchLog, action.message],
      };
    case "batch_stage_message":
      return {
        ...state,
        currentBatchStage: action.message,
        batchStageHistory: pushHistory(state.batchStageHistory, action.message),
      };
    case "batch_started": {
      const itemStates = createBatchItemStateRecord(action.items);
      return {
        ...state,
        stage: "executing",
        itemStates,
        activeItemId: action.items[0]?.id ?? null,
      };
    }
    case "item_started": {
      const itemStates = updateBatchItemStateRecord(state.itemStates, action.itemId, { status: "img_generating" });
      return {
        ...state,
        itemStates,
        activeItemId: pickNextActiveOnStart(state.activeItemId, state.itemStates, action.itemId),
      };
    }
    case "item_progress_received": {
      let itemStates = appendBatchItemProgress(state.itemStates, action.itemId, action.message);
      if (action.message.includes("Code Agent")) {
        itemStates = updateBatchItemStateRecord(itemStates, action.itemId, { status: "code_generating" });
      }
      return {
        ...state,
        itemStates,
      };
    }
    case "item_image_ready":
      return {
        ...state,
        itemStates: applyBatchItemImage(state.itemStates, action.itemId, action.image, action.index, action.prompt),
        activeItemId: state.activeItemId ?? action.itemId,
      };
    case "item_agent_stream":
      return {
        ...state,
        itemStates: appendBatchItemAgentLog(state.itemStates, action.itemId, {
          text: action.chunk,
          source: action.source,
          channel: action.channel,
          model: action.model,
        }),
      };
    case "item_approval_pending":
      return {
        ...state,
        itemStates: updateBatchItemStateRecord(state.itemStates, action.itemId, {
          status: "approval_pending",
          approvalSummary: action.summary,
          approvalRequests: action.requests,
        }),
        activeItemId: state.activeItemId ?? action.itemId,
      };
    case "item_done": {
      const itemStates = updateBatchItemStateRecord(state.itemStates, action.itemId, { status: "done" });
      return {
        ...state,
        itemStates,
        activeItemId: pickNextActiveOnDone(state.activeItemId, action.itemId, itemStates),
      };
    }
    case "item_error":
      return {
        ...state,
        itemStates: updateBatchItemStateRecord(state.itemStates, action.itemId, {
          status: "error",
          error: action.message,
          errorTrace: action.traceback,
        }),
      };
    case "batch_done":
      return {
        ...state,
        stage: "done",
        batchResult: { success: action.success, error: action.error },
      };
    case "workflow_cancelled":
      if (state.stage === "cancelled") {
        return state;
      }
      return {
        ...state,
        stage: "cancelled",
        batchLog: [...state.batchLog, action.message ?? "已取消当前生成"],
        workflowErrorMessage: null,
        itemStates: Object.fromEntries(
          Object.entries(state.itemStates).map(([id, itemState]) => [
            id,
            itemState.status === "done" || itemState.status === "error"
              ? itemState
              : { ...itemState, status: "cancelled" as const },
          ]),
        ) as BatchItemStateRecord,
      };
    case "workflow_failed":
      return {
        ...state,
        stage: "error",
        workflowErrorMessage: action.message,
      };
    case "workflow_reset":
      return createInitialBatchRuntimeState();
    case "active_item_set":
      return {
        ...state,
        activeItemId: action.itemId,
      };
    case "approval_busy_set":
      return {
        ...state,
        approvalBusyActionId: action.actionId,
      };
    case "approval_request_updated": {
      const itemStates = Object.fromEntries(
        Object.entries(state.itemStates).map(([id, itemState]) => {
          const nextRequests = itemState.approvalRequests.map((req) =>
            req.action_id === action.actionId ? action.request : req,
          );
          return [id, { ...itemState, approvalRequests: nextRequests }];
        }),
      ) as BatchItemStateRecord;
      return {
        ...state,
        itemStates,
      };
    }
    case "item_state_patched":
      return {
        ...state,
        itemStates: updateBatchItemStateRecord(state.itemStates, action.itemId, action.patch),
      };
    case "item_stage_message":
      return {
        ...state,
        itemStates: applyBatchItemStageMessage(state.itemStates, action.itemId, action.message),
      };
    default:
      return state;
  }
}
