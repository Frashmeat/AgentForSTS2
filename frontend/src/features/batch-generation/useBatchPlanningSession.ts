// BatchModePage 的 WebSocket 会话生命周期封装。
// 收纳 socketRef + startPlanning / confirmPlan / cancelBatch / closeSocket / _registerBatchHandlers。
// 主组件只负责传入 dispatch + 状态 setters + ref，不再直接接触 BatchSocket。

import { useRef, type Dispatch, type MutableRefObject } from "react";

import { BatchSocket, ModPlan, PlanItem } from "../../lib/batch_ws";
import { WORKFLOW_CANCELLED_MESSAGE, isWorkflowCancellation, resolveWorkflowErrorMessage } from "../../shared/error.ts";
import type { PlanReviewPayload } from "../../shared/types/workflow.ts";
import type { WorkflowLogEntry } from "../../shared/workflowLog.ts";
import { openBatchPlanningSocket } from "./planningSession";
import {
  reconcileBundleDecisionRecord,
  type BatchItemState as ItemState,
  type BatchRuntimeAction,
  type BundleDecisionRecord,
  type ReviewStrictness,
} from "./state.ts";
import {
  PLAN_BUNDLE_DECISIONS_STORAGE_KEY,
  PLAN_ITEMS_STORAGE_KEY,
  PLAN_REVIEW_STORAGE_KEY,
  PLAN_REVIEW_STRICTNESS_STORAGE_KEY,
  PLAN_STORAGE_KEY,
  type ReviewFeedback,
} from "./view-constants.ts";
import { normalizeReviewStrictness, summarizePlanReview, writeJsonStorage, writeTextStorage } from "./view-helpers.ts";

export interface UseBatchPlanningSessionOptions {
  dispatchRuntime: Dispatch<BatchRuntimeAction>;
  setReviewError: (value: string | null) => void;
  setReviewFeedback: (value: ReviewFeedback | null) => void;
  setReviewFocusItemId: (value: string | null) => void;
  setPlan: (plan: ModPlan | null) => void;
  setEditedItems: (items: PlanItem[]) => void;
  updateItem: (id: string, patch: Partial<ItemState>) => void;
  appendProgress: (id: string, msg: string) => void;
  appendAgent: (id: string, entry: WorkflowLogEntry) => void;
  addImage: (id: string, b64: string, index: number, prompt: string) => void;
  autoSelectRef: MutableRefObject<boolean>;
}

export interface StartPlanningArgs {
  requirements: string;
  projectRoot: string;
  reviewStrictness: ReviewStrictness;
  activeItemId: string | null;
}

export interface ConfirmPlanArgs {
  plan: ModPlan;
  editedItems: PlanItem[];
  projectRoot: string;
  reviewStrictness: ReviewStrictness;
  planReview: PlanReviewPayload | null;
  bundleDecisions: BundleDecisionRecord;
}

export interface UseBatchPlanningSessionResult {
  socketRef: MutableRefObject<BatchSocket | null>;
  startPlanning: (args: StartPlanningArgs) => Promise<void>;
  confirmPlan: (args: ConfirmPlanArgs) => Promise<void>;
  cancelBatch: () => void;
  closeSocket: () => void;
}

export function useBatchPlanningSession({
  dispatchRuntime,
  setReviewError,
  setReviewFeedback,
  setReviewFocusItemId,
  setPlan,
  setEditedItems,
  updateItem,
  appendProgress,
  appendAgent,
  addImage,
  autoSelectRef,
}: UseBatchPlanningSessionOptions): UseBatchPlanningSessionResult {
  const socketRef = useRef<BatchSocket | null>(null);

  function registerBatchHandlers(
    ws: BatchSocket,
    ctx: { reviewStrictness: ReviewStrictness; activeItemId: string | null },
  ) {
    ws.on("planning", () => dispatchRuntime({ type: "batch_log_appended", message: "正在规划 Mod..." }));
    ws.on("plan_ready", (d) => {
      setPlan(d.plan);
      setEditedItems(d.plan.items);
      setReviewError(null);
      setReviewFeedback(d.review ? summarizePlanReview(d.review, d.plan.items).feedback : null);
      setReviewFocusItemId(d.review ? summarizePlanReview(d.review, d.plan.items).focusItemId : null);
      const nextDecisions = reconcileBundleDecisionRecord(d.review ?? null);
      dispatchRuntime({ type: "plan_ready_received", review: d.review ?? null, decisions: nextDecisions });
      try {
        writeJsonStorage(PLAN_STORAGE_KEY, d.plan);
        writeJsonStorage(PLAN_ITEMS_STORAGE_KEY, d.plan.items);
        writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, d.review ?? null);
        writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, nextDecisions);
        writeTextStorage(
          PLAN_REVIEW_STRICTNESS_STORAGE_KEY,
          normalizeReviewStrictness(d.review?.strictness ?? ctx.reviewStrictness),
        );
      } catch {}
    });
    ws.on("batch_progress", (d) => dispatchRuntime({ type: "batch_log_appended", message: d.message }));
    ws.on("stage_update", (d) => {
      if (d.item_id) {
        dispatchRuntime({ type: "item_stage_message", itemId: d.item_id, message: d.message });
        return;
      }
      dispatchRuntime({ type: "batch_stage_message", message: d.message });
    });
    ws.on("batch_started", (d) => {
      dispatchRuntime({ type: "batch_started", items: d.items });
    });
    ws.on("item_started", (d) => {
      dispatchRuntime({ type: "item_started", itemId: d.item_id });
    });
    ws.on("item_progress", (d) => {
      appendProgress(d.item_id, d.message);
      if (d.message.includes("Code Agent")) {
        updateItem(d.item_id, { status: "code_generating" });
      }
    });
    ws.on("item_image_ready", (d) => {
      addImage(d.item_id, d.image, d.index, d.prompt);
      if (autoSelectRef.current) {
        ws.send({ action: "select_image", item_id: d.item_id, index: 0 });
        updateItem(d.item_id, { status: "code_generating" });
      }
    });
    ws.on("item_agent_stream", (d) => {
      appendAgent(d.item_id, {
        text: d.chunk,
        source: d.source,
        channel: d.channel,
        model: d.model,
      });
    });
    ws.on("item_approval_pending", (d) => {
      updateItem(d.item_id, {
        status: "approval_pending",
        approvalSummary: d.summary,
        approvalRequests: d.requests,
      });
      if (ctx.activeItemId === null) {
        dispatchRuntime({ type: "active_item_set", itemId: d.item_id });
      }
    });
    ws.on("item_done", (d) => {
      dispatchRuntime({ type: "item_done", itemId: d.item_id });
    });
    ws.on("item_error", (d) => {
      updateItem(d.item_id, {
        status: "error",
        error: resolveWorkflowErrorMessage(d),
        errorTrace: d.traceback ?? null,
      });
    });
    ws.on("batch_done", (d) => {
      socketRef.current = null;
      dispatchRuntime({ type: "batch_done", success: d.success_count, error: d.error_count });
    });
    ws.on("cancelled", (d) => {
      socketRef.current = null;
      dispatchRuntime({
        type: "workflow_cancelled",
        message: resolveWorkflowErrorMessage(d, WORKFLOW_CANCELLED_MESSAGE),
      });
    });
    ws.on("error", (d) => {
      socketRef.current = null;
      if (isWorkflowCancellation(d)) {
        dispatchRuntime({
          type: "workflow_cancelled",
          message: resolveWorkflowErrorMessage(d, WORKFLOW_CANCELLED_MESSAGE),
        });
        return;
      }
      dispatchRuntime({ type: "workflow_failed", message: resolveWorkflowErrorMessage(d) });
    });
  }

  async function startPlanning({ requirements, projectRoot, reviewStrictness, activeItemId }: StartPlanningArgs) {
    if (!requirements.trim()) return;
    dispatchRuntime({ type: "planning_started" });
    setReviewError(null);
    setReviewFeedback(null);
    setReviewFocusItemId(null);
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, {});
    setPlan(null);

    const ws = new BatchSocket();
    socketRef.current = ws;
    registerBatchHandlers(ws, { reviewStrictness, activeItemId });
    const started = await openBatchPlanningSocket(ws, {
      requirements,
      projectRoot,
      onOpenError(message) {
        socketRef.current = null;
        dispatchRuntime({ type: "workflow_failed", message });
      },
    });
    if (!started) {
      return;
    }
  }

  async function confirmPlan({
    plan,
    editedItems,
    projectRoot,
    reviewStrictness,
    planReview,
    bundleDecisions,
  }: ConfirmPlanArgs) {
    const itemsForStorage = editedItems.map((it) => ({ ...it, provided_image_b64: undefined }));
    writeJsonStorage(PLAN_ITEMS_STORAGE_KEY, itemsForStorage);
    writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, planReview);
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, bundleDecisions);
    writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, reviewStrictness);

    if (!socketRef.current) {
      // 恢复的规划：重新建连接，直接跳到执行
      const ws = new BatchSocket();
      socketRef.current = ws;
      registerBatchHandlers(ws, { reviewStrictness, activeItemId: null });
      const started = await openBatchPlanningSocket(ws, {
        payload: {
          action: "start_with_plan",
          project_root: projectRoot,
          plan: { ...plan, items: editedItems },
          review_strictness: reviewStrictness,
          bundle_decisions: bundleDecisions,
        },
        onOpenError(message) {
          socketRef.current = null;
          dispatchRuntime({ type: "workflow_failed", message });
        },
      });
      if (!started) {
        return;
      }
      dispatchRuntime({ type: "review_bundles_confirmed" });
    } else {
      dispatchRuntime({ type: "review_bundles_confirmed" });
      socketRef.current.send({
        action: "confirm_plan",
        plan: { ...plan, items: editedItems },
        review_strictness: reviewStrictness,
        bundle_decisions: bundleDecisions,
      });
    }
  }

  function cancelBatch() {
    const socket = socketRef.current;
    if (socket) {
      try {
        socket.send({ action: "cancel" });
      } catch {}
      setTimeout(() => {
        socket.close();
      }, 100);
    }
    socketRef.current = null;
    dispatchRuntime({ type: "workflow_cancelled", message: WORKFLOW_CANCELLED_MESSAGE });
  }

  function closeSocket() {
    socketRef.current?.close();
    socketRef.current = null;
  }

  return { socketRef, startPlanning, confirmPlan, cancelBatch, closeSocket };
}
