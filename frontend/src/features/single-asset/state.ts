import type { ApprovalRequest } from "../../shared/api/approvals";
import type { Stage } from "./model";

export interface SingleAssetWorkflowState {
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
  approvalBusyActionId: string | null;
  errorMsg: string | null;
  errorTrace: string | null;
}

export type SingleAssetWorkflowAction =
  | { type: "workflow_started"; imageMode: "ai" | "upload" }
  | { type: "prompt_preview_received"; prompt: string; negativePrompt: string; fallbackWarning: string | null }
  | { type: "image_ready_received"; index: number; image: string; prompt: string; batchOffset: number }
  | { type: "approval_pending_received"; summary: string; requests: ApprovalRequest[] }
  | { type: "workflow_failed"; message: string; traceback: string | null }
  | { type: "workflow_reset" }
  | { type: "stage_changed"; stage: Stage }
  | { type: "gen_log_appended"; message: string }
  | { type: "agent_log_appended"; message: string }
  | { type: "flow_stage_pushed"; message: string }
  | { type: "agent_stage_pushed"; message: string }
  | { type: "prompt_confirmed" }
  | { type: "image_selected" }
  | { type: "generate_more_requested"; batchOffset: number }
  | { type: "prompt_preview_changed"; value: string }
  | { type: "negative_prompt_changed"; value: string }
  | { type: "current_prompt_changed"; value: string }
  | { type: "show_more_prompt_toggled" }
  | { type: "approval_busy_set"; actionId: string | null }
  | { type: "approval_requests_updated"; requests: ApprovalRequest[] };

export function createInitialSingleAssetWorkflowState(): SingleAssetWorkflowState {
  return {
    stage: "input",
    images: [],
    pendingSlots: 0,
    promptPreview: "",
    negativePrompt: "",
    promptFallbackWarn: null,
    currentPrompt: "",
    showMorePrompt: false,
    genLog: [],
    agentLog: [],
    flowStageCurrent: null,
    flowStageHistory: [],
    agentStageCurrent: null,
    agentStageHistory: [],
    approvalSummary: "",
    approvalRequests: [],
    approvalBusyActionId: null,
    errorMsg: null,
    errorTrace: null,
  };
}

function pushHistory(history: string[], message: string): string[] {
  return history[history.length - 1] === message ? history : [...history, message];
}

function createClearedWorkflowState(imageMode: "ai" | "upload"): SingleAssetWorkflowState {
  const initial = createInitialSingleAssetWorkflowState();
  return {
    ...initial,
    stage: imageMode === "upload" ? "input" : "generating_image",
  };
}

export function singleAssetWorkflowReducer(
  state: SingleAssetWorkflowState,
  action: SingleAssetWorkflowAction,
): SingleAssetWorkflowState {
  switch (action.type) {
    case "workflow_started":
      return createClearedWorkflowState(action.imageMode);
    case "prompt_preview_received":
      return {
        ...state,
        stage: "confirm_prompt",
        promptPreview: action.prompt,
        currentPrompt: action.prompt,
        negativePrompt: action.negativePrompt,
        promptFallbackWarn: action.fallbackWarning,
      };
    case "image_ready_received": {
      const images = [...state.images];
      images[action.batchOffset + action.index] = action.image;
      return {
        ...state,
        stage: "pick_image",
        images,
        pendingSlots: 0,
        currentPrompt: action.prompt,
        showMorePrompt: false,
      };
    }
    case "approval_pending_received":
      return {
        ...state,
        stage: "approval_pending",
        approvalSummary: action.summary,
        approvalRequests: action.requests,
        agentLog: [...state.agentLog, "已生成待审批动作，等待用户审批后继续执行。"],
      };
    case "workflow_failed":
      return {
        ...state,
        stage: "error",
        errorMsg: action.message,
        errorTrace: action.traceback,
      };
    case "workflow_reset":
      return createInitialSingleAssetWorkflowState();
    case "stage_changed":
      return {
        ...state,
        stage: action.stage,
      };
    case "gen_log_appended":
      return {
        ...state,
        genLog: [...state.genLog, action.message],
      };
    case "agent_log_appended":
      return {
        ...state,
        agentLog: [...state.agentLog, action.message],
      };
    case "flow_stage_pushed":
      return {
        ...state,
        flowStageCurrent: action.message,
        flowStageHistory: pushHistory(state.flowStageHistory, action.message),
      };
    case "agent_stage_pushed":
      return {
        ...state,
        agentStageCurrent: action.message,
        agentStageHistory: pushHistory(state.agentStageHistory, action.message),
      };
    case "prompt_confirmed":
      return {
        ...state,
        stage: "generating_image",
      };
    case "image_selected":
      return {
        ...state,
        stage: "agent_running",
      };
    case "generate_more_requested":
      return {
        ...state,
        pendingSlots: 1,
        showMorePrompt: false,
      };
    case "prompt_preview_changed":
      return {
        ...state,
        promptPreview: action.value,
      };
    case "negative_prompt_changed":
      return {
        ...state,
        negativePrompt: action.value,
      };
    case "current_prompt_changed":
      return {
        ...state,
        currentPrompt: action.value,
      };
    case "show_more_prompt_toggled":
      return {
        ...state,
        showMorePrompt: !state.showMorePrompt,
      };
    case "approval_busy_set":
      return {
        ...state,
        approvalBusyActionId: action.actionId,
      };
    case "approval_requests_updated":
      return {
        ...state,
        approvalRequests: action.requests,
      };
    default:
      return state;
  }
}
