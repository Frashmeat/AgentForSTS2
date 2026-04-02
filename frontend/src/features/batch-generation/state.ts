import type { ApprovalRequest } from "../../shared/api/index.ts";

export type BatchItemStatus =
  | "pending"
  | "img_generating"
  | "awaiting_selection"
  | "approval_pending"
  | "code_generating"
  | "done"
  | "error";

export interface BatchItemState {
  status: BatchItemStatus;
  currentStage: string | null;
  stageHistory: string[];
  progress: string[];
  images: string[];
  agentLog: string[];
  error: string | null;
  errorTrace: string | null;
  currentPrompt: string;
  showMorePrompt: boolean;
  approvalSummary: string;
  approvalRequests: ApprovalRequest[];
}

export type BatchItemStateRecord = Record<string, BatchItemState>;

export type BatchStage = "input" | "planning" | "review_plan" | "executing" | "done" | "error";

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
}

export type BatchRuntimeAction =
  | { type: "planning_started" }
  | { type: "stage_set"; stage: BatchStage }
  | { type: "plan_ready_received" }
  | { type: "batch_log_appended"; message: string }
  | { type: "batch_stage_message"; message: string }
  | { type: "batch_started"; items: Array<{ id: string }> }
  | { type: "item_started"; itemId: string }
  | { type: "item_progress_received"; itemId: string; message: string }
  | { type: "item_image_ready"; itemId: string; image: string; index: number; prompt: string }
  | { type: "item_agent_stream"; itemId: string; chunk: string }
  | { type: "item_approval_pending"; itemId: string; summary: string; requests: ApprovalRequest[] }
  | { type: "item_done"; itemId: string }
  | { type: "item_error"; itemId: string; message: string; traceback: string | null }
  | { type: "batch_done"; success: number; error: number }
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
  chunk: string,
): BatchItemStateRecord {
  const current = record[id] ?? createDefaultBatchItemState();
  return {
    ...record,
    [id]: {
      ...current,
      agentLog: [...current.agentLog, chunk],
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
        stage: "review_plan",
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
        itemStates: appendBatchItemAgentLog(state.itemStates, action.itemId, action.chunk),
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
          const nextStatus =
            itemState.status === "approval_pending" &&
            nextRequests.length > 0 &&
            nextRequests.every((req) => req.status === "succeeded")
              ? "done"
              : itemState.status;
          return [id, { ...itemState, approvalRequests: nextRequests, status: nextStatus }];
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
