import { useState, useCallback, useRef, useEffect, useReducer } from "react";
import { AlertTriangle, Loader2, RotateCcw, Sparkles, StopCircle } from "lucide-react";

import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "../../lib/approvals";
import { BatchSocket, PlanItem, ModPlan } from "../../lib/batch_ws";
import { AgentLog } from "../../components/AgentLog";
import { ProjectRootField } from "../../components/ProjectRootField";
import { loadAppConfig } from "../../shared/api/config";
import { runApprovalAction } from "../../shared/approvalAction.ts";
import { reviewModPlan } from "../../shared/api/workflow.ts";
import {
  WORKFLOW_CANCELLED_MESSAGE,
  isWorkflowCancellation,
  resolveErrorMessage,
  resolveWorkflowErrorMessage,
} from "../../shared/error.ts";
import type { PlanReviewPayload } from "../../shared/types/workflow.ts";
import type { WorkflowLogEntry } from "../../shared/workflowLog.ts";
import { useResolvedWorkspaceFeatureProps } from "../workspace/WorkspaceContext.tsx";
import type { WorkspaceFeatureAdapterProps, WorkspaceFeatureProps } from "../workspace/types.ts";
import { canProceedBatchApproval, markBatchApprovalResuming, resumeBatchApprovalWorkflow } from "./approval";
import { ExecutionView } from "./ExecutionView.tsx";
import { openBatchPlanningSocket } from "./planningSession";
import { ReviewBundles } from "./ReviewBundles.tsx";
import { ReviewPlan } from "./ReviewPlan.tsx";
import {
  batchWorkflowReducer,
  createInitialBatchRuntimeState,
  canProceedFromBundleReview,
  reconcileBundleDecisionRecord,
  summarizeBundleDecisionProgress,
  type BatchItemState as ItemState,
  type BundleDecisionRecord,
  type BundleDecisionStatus,
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
import {
  canProceedFromEditedItemReview,
  normalizeReviewStrictness,
  readJsonStorage,
  summarizePlanReview,
  writeJsonStorage,
  writeTextStorage,
} from "./view-helpers.ts";

// ── 主组件 ────────────────────────────────────────────────────────────────────

function BatchModePage({
  onRequestExecution,
  onStatusNotice,
  knowledgeStatus: _knowledgeStatus,
  onOpenKnowledgeGuide: _onOpenKnowledgeGuide,
  onOpenSettings: _onOpenSettings,
}: WorkspaceFeatureProps) {
  const [requirements, setRequirements] = useState("");
  const [projectRoot, setProjectRoot] = useState("");

  function deriveServerWorkspaceProjectName() {
    const normalizedProjectRoot = projectRoot.trim().replace(/\\/g, "/");
    if (!normalizedProjectRoot) {
      return "";
    }
    const segments = normalizedProjectRoot.split("/").filter(Boolean);
    return segments[segments.length - 1] ?? "";
  }

  useEffect(() => {
    loadAppConfig()
      .then((config) => {
        if (config?.default_project_root) {
          setProjectRoot(String(config.default_project_root));
        }
      })
      .catch(() => {});
  }, []);

  const [plan, setPlan] = useState<ModPlan | null>(() => {
    return readJsonStorage<ModPlan | null>(PLAN_STORAGE_KEY, null);
  });
  const [editedItems, setEditedItems] = useState<PlanItem[]>(() => {
    return readJsonStorage<PlanItem[]>(PLAN_ITEMS_STORAGE_KEY, []);
  });
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  const [reviewFeedback, setReviewFeedback] = useState<ReviewFeedback | null>(null);
  const [reviewFocusItemId, setReviewFocusItemId] = useState<string | null>(null);
  const [runtimeState, dispatchRuntime] = useReducer(batchWorkflowReducer, undefined, () => ({
    ...createInitialBatchRuntimeState(),
    planReview: readJsonStorage<PlanReviewPayload | null>(PLAN_REVIEW_STORAGE_KEY, null),
    bundleDecisions: readJsonStorage<BundleDecisionRecord>(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, {}),
  }));
  const {
    stage,
    activeItemId,
    itemStates,
    batchLog,
    currentBatchStage,
    batchStageHistory,
    workflowErrorMessage,
    batchResult,
    approvalBusyActionId,
    planReview,
    reviewStrictness,
    bundleDecisions,
  } = runtimeState;
  const itemStatesRef = useRef<Record<string, ItemState>>({});

  const [autoSelectFirst, setAutoSelectFirst] = useState(false);
  const autoSelectRef = useRef(false);
  useEffect(() => {
    autoSelectRef.current = autoSelectFirst;
  }, [autoSelectFirst]);
  const socketRef = useRef<BatchSocket | null>(null);

  useEffect(() => {
    itemStatesRef.current = itemStates;
  }, [itemStates]);

  useEffect(() => {
    const nextDecisions = reconcileBundleDecisionRecord(planReview, bundleDecisions);
    const sameKeys =
      Object.keys(nextDecisions).length === Object.keys(bundleDecisions).length &&
      Object.entries(nextDecisions).every(([key, value]) => bundleDecisions[key] === value);
    if (!sameKeys) {
      dispatchRuntime({ type: "bundle_decisions_set", decisions: nextDecisions });
      writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, nextDecisions);
    }
  }, [planReview, bundleDecisions]);

  // ── State updater helpers ─────────────────────────────────────────────────

  const updateItem = useCallback((id: string, patch: Partial<ItemState>) => {
    dispatchRuntime({ type: "item_state_patched", itemId: id, patch });
  }, []);

  const appendProgress = useCallback((id: string, msg: string) => {
    dispatchRuntime({ type: "item_progress_received", itemId: id, message: msg });
  }, []);

  const appendAgent = useCallback((id: string, entry: WorkflowLogEntry) => {
    dispatchRuntime({
      type: "item_agent_stream",
      itemId: id,
      chunk: entry.text,
      source: entry.source,
      channel: entry.channel,
      model: entry.model,
    });
  }, []);

  const addImage = useCallback((id: string, b64: string, index: number, prompt: string) => {
    dispatchRuntime({
      type: "item_image_ready",
      itemId: id,
      image: b64,
      index,
      prompt,
    });
  }, []);

  // ── Start ─────────────────────────────────────────────────────────────────

  async function startPlanning() {
    if (!requirements.trim()) return;
    dispatchRuntime({ type: "planning_started" });
    setReviewError(null);
    setReviewFeedback(null);
    setReviewFocusItemId(null);
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, {});
    setPlan(null);

    const ws = new BatchSocket();
    socketRef.current = ws;
    _registerBatchHandlers(ws);
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

  function _registerBatchHandlers(ws: BatchSocket) {
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
          normalizeReviewStrictness(d.review?.strictness ?? reviewStrictness),
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
      if (activeItemId === null) {
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

  async function confirmPlan() {
    if (!plan) return;
    const itemsForStorage = editedItems.map((it) => ({ ...it, provided_image_b64: undefined }));
    writeJsonStorage(PLAN_ITEMS_STORAGE_KEY, itemsForStorage);
    writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, planReview);
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, bundleDecisions);
    writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, reviewStrictness);

    if (!socketRef.current) {
      // 恢复的规划：重新建连接，直接跳到执行
      const ws = new BatchSocket();
      socketRef.current = ws;
      _registerBatchHandlers(ws);
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

  function updateEditedItems(items: PlanItem[]) {
    setEditedItems(items);
    writeJsonStorage(PLAN_ITEMS_STORAGE_KEY, items);
  }

  async function refreshPlanReview(
    items: PlanItem[] = editedItems,
    strictness: ReviewStrictness = reviewStrictness,
    decisions: BundleDecisionRecord = bundleDecisions,
  ): Promise<PlanReviewPayload | null> {
    if (!plan) {
      return null;
    }
    setReviewBusy(true);
    setReviewError(null);
    setReviewFeedback({ tone: "info", message: "正在重新检查当前计划..." });
    try {
      const review = await reviewModPlan({
        plan: { ...plan, items },
        strictness,
        bundle_decisions: decisions,
      });
      const nextDecisions = reconcileBundleDecisionRecord(review);
      dispatchRuntime({ type: "review_updated", review, decisions: nextDecisions });
      const summary = summarizePlanReview(review, items);
      setReviewFeedback(summary.feedback);
      setReviewFocusItemId(summary.focusItemId);
      writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, review);
      writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, nextDecisions);
      writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, normalizeReviewStrictness(review.strictness));
      return review;
    } catch (error) {
      setReviewError(resolveErrorMessage(error));
      setReviewFeedback(null);
      return null;
    } finally {
      setReviewBusy(false);
    }
  }

  async function handleConfirmItemsReview() {
    const review = await refreshPlanReview();
    if (!review) {
      return;
    }
    if (canProceedFromEditedItemReview(review, editedItems)) {
      setReviewFeedback({ tone: "success", message: "Item 复核已通过，进入执行策略决策。" });
      dispatchRuntime({ type: "review_items_confirmed" });
    }
  }

  async function handleConfirmBundleReview() {
    const review = planReview ?? (await refreshPlanReview());
    if (!review) {
      return;
    }
    if (!canProceedFromEditedItemReview(review, editedItems)) {
      setReviewFeedback({ tone: "warning", message: "仍有 item 说明未补齐，已返回 Item 复核阶段。" });
      dispatchRuntime({ type: "stage_set", stage: "review_items" });
      return;
    }
    if (!canProceedFromBundleReview(review, bundleDecisions)) {
      const progress = summarizeBundleDecisionProgress(review, bundleDecisions);
      const pendingParts = [
        progress.unresolved > 0 ? `${progress.unresolved} 个 bundle 待决策` : null,
        progress.splitRequested > 0 ? `${progress.splitRequested} 个 bundle 已要求拆分但尚未完成重算` : null,
        progress.needsItemRevision > 0 ? `${progress.needsItemRevision} 个 bundle 仍需回到 Item 复核` : null,
      ].filter(Boolean);
      setReviewFeedback({
        tone: "warning",
        message: `执行策略仍需处理：${pendingParts.join("，")}。`,
      });
      dispatchRuntime({ type: "stage_set", stage: "review_bundles" });
      return;
    }
    setReviewFeedback({ tone: "success", message: "执行策略复核通过，开始进入执行阶段。" });
    requestExecutionStart();
  }

  function handleReviewStrictnessChange(nextStrictness: ReviewStrictness) {
    dispatchRuntime({ type: "review_strictness_set", strictness: nextStrictness });
    writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, nextStrictness);
    if (plan) {
      void refreshPlanReview(editedItems, nextStrictness, {});
    }
  }

  function handleBundleDecisionChange(bundleKey: string, decision: BundleDecisionStatus) {
    const next = { ...bundleDecisions, [bundleKey]: decision };
    dispatchRuntime({ type: "bundle_decisions_set", decisions: next });
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, next);
  }

  async function handleBundleSplitRequest(bundleKey: string) {
    const nextDecisions = { ...bundleDecisions, [bundleKey]: "split_requested" as const };
    dispatchRuntime({ type: "bundle_decisions_set", decisions: nextDecisions });
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, nextDecisions);
    setReviewFeedback({ tone: "info", message: "已记录拆分请求，正在按更保守口径重算 bundle..." });
    const review = await refreshPlanReview(editedItems, reviewStrictness, nextDecisions);
    if (!review) {
      return;
    }
    const progress = summarizeBundleDecisionProgress(review, reconcileBundleDecisionRecord(review));
    setReviewFeedback({
      tone: progress.blocking === 0 ? "success" : "warning",
      message:
        progress.blocking === 0
          ? "已按拆分请求重算执行策略，当前可以直接进入执行。"
          : "已按拆分请求重算执行策略，请继续处理剩余 bundle 决策。",
    });
  }

  function handleBundleReturnToItems(bundleKey: string, itemIds: string[]) {
    const next = { ...bundleDecisions, [bundleKey]: "needs_item_revision" as const };
    dispatchRuntime({ type: "bundle_decisions_set", decisions: next });
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, next);
    setReviewFocusItemId(itemIds[0] ?? null);
    setReviewFeedback({
      tone: "warning",
      message: "已返回 Item 复核阶段，请补充依赖原因、范围边界或验收说明后重新检查。",
    });
    dispatchRuntime({ type: "stage_set", stage: "review_items" });
  }

  function requestExecutionStart() {
    if (!plan) {
      return;
    }

    const executeLocal = () => {
      void confirmPlan();
    };
    if (!onRequestExecution) {
      executeLocal();
      return;
    }
    onRequestExecution({
      title: "执行 Mod 规划",
      tab: "batch",
      jobType: "batch_generate",
      createdFrom: "batch_generation",
      inputSummary: plan.summary || requirements.trim() || plan.mod_name,
      requiresCodeAgent: true,
      requiresImageAi: editedItems.some((item) => item.needs_image && !item.provided_image_b64),
      serverWorkspaceProjectName: deriveServerWorkspaceProjectName(),
      items: editedItems.map((item, index) => ({
        item_type: item.type,
        input_summary: item.description || item.name,
        input_payload: {
          item_index: index,
          item_name: item.name,
          description: item.description,
          goal: item.goal,
          detailed_description: item.detailed_description,
          scope_boundary: item.scope_boundary,
          dependency_reason: item.dependency_reason,
          acceptance_notes: item.acceptance_notes,
          affected_targets: item.affected_targets,
          coupling_kind: item.coupling_kind,
          image_description: item.image_description,
          implementation_notes: item.implementation_notes,
          depends_on: item.depends_on,
        },
      })),
      serverUploads: editedItems.flatMap((item, index) =>
        item.provided_image_b64
          ? [
              {
                itemIndex: index,
                fileName: `${item.name || "uploaded"}.png`,
                contentBase64: item.provided_image_b64,
                mimeType: "image/png",
              },
            ]
          : [],
      ),
      runLocal: executeLocal,
    });
  }

  function handleSelectImage(itemId: string, index: number) {
    if (!socketRef.current) return;
    socketRef.current.send({ action: "select_image", item_id: itemId, index });
    updateItem(itemId, { status: "code_generating" });
    const nextAwaiting = editedItems.find(
      (it) => it.id !== itemId && itemStatesRef.current[it.id]?.status === "awaiting_selection",
    );
    if (nextAwaiting) dispatchRuntime({ type: "active_item_set", itemId: nextAwaiting.id });
  }

  function handleRetryItem(itemId: string) {
    if (!socketRef.current) return;
    socketRef.current.send({ action: "retry_item", item_id: itemId });
    updateItem(itemId, {
      status: "img_generating",
      error: null,
      errorTrace: null,
      progress: [],
      agentLog: [],
      agentLogEntries: [],
      currentAgentModel: null,
      images: [],
    });
  }

  function handleGenerateMore(itemId: string) {
    if (!socketRef.current) return;
    const state = itemStatesRef.current[itemId];
    socketRef.current.send({
      action: "generate_more",
      item_id: itemId,
      prompt: state?.currentPrompt,
    });
    updateItem(itemId, { status: "img_generating", showMorePrompt: false });
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

  function reset() {
    socketRef.current?.close();
    socketRef.current = null;
    dispatchRuntime({ type: "workflow_reset" });
    setPlan(null);
    setEditedItems([]);
    setReviewError(null);
    setReviewFeedback(null);
    setReviewFocusItemId(null);
    writeJsonStorage(PLAN_BUNDLE_DECISIONS_STORAGE_KEY, {});
  }

  async function handleApprovalAction(actionId: string, action: (id: string) => Promise<ApprovalRequest>) {
    await runApprovalAction({
      actionId,
      action,
      onBusyChange(actionIdOrNull) {
        dispatchRuntime({ type: "approval_busy_set", actionId: actionIdOrNull });
      },
      onSuccess(updated) {
        dispatchRuntime({ type: "approval_request_updated", actionId, request: updated });
      },
      onError(message) {
        dispatchRuntime({ type: "workflow_failed", message });
      },
    });
  }

  function handleProceedApproval(itemId: string) {
    const socket = socketRef.current;
    const state = itemStatesRef.current[itemId];
    if (!socket || !state || !canProceedBatchApproval(state.approvalRequests)) {
      return;
    }

    dispatchRuntime({
      type: "item_state_patched",
      itemId,
      patch: markBatchApprovalResuming({ [itemId]: state }, itemId)[itemId],
    });
    resumeBatchApprovalWorkflow(socket, itemId);
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="space-y-5">
      {/* 输入阶段 */}
      {stage === "input" && (
        <div className="workspace-surface rounded-2xl p-5 space-y-4">
          <h2 className="font-semibold text-slate-800">描述你的 Mod 需求</h2>
          <p className="text-xs text-slate-400">
            用自然语言描述整个 Mod 的内容，AI 会自动规划需要哪些卡牌、遗物、机制，并逐一创建。
          </p>
          <textarea
            value={requirements}
            onChange={(e) => setRequirements(e.target.value)}
            rows={6}
            placeholder={
              "例如：\n我想做一个暗法师角色，主题是腐化和献祭。\n包含3张卡牌（攻击、技能、力量各一张），\n2个遗物（战斗开始触发），\n以及一个腐化叠层的 buff 机制。"
            }
            className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-violet-400 focus:ring-1 focus:ring-violet-100 resize-none"
          />
          <ProjectRootField
            value={projectRoot}
            placeholder="E:/STS2mod"
            showCreateAction={false}
            onStatusNotice={onStatusNotice}
            onChange={setProjectRoot}
          />
          <button
            onClick={startPlanning}
            disabled={!requirements.trim() || !projectRoot.trim()}
            className="w-full py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center justify-center gap-2"
          >
            <Sparkles size={15} />
            规划 Mod
          </button>
          {plan && (
            <button
              onClick={() => dispatchRuntime({ type: "stage_set", stage: "review_items" })}
              className="w-full py-2 rounded-lg border border-violet-200 text-violet-700 text-sm hover:bg-violet-50 transition-colors"
            >
              恢复上次规划：{plan.mod_name}
            </button>
          )}
        </div>
      )}

      {/* 规划中 */}
      {stage === "planning" && (
        <div className="rounded-xl border border-slate-200 bg-white p-5 space-y-3">
          <div className="flex items-center gap-2.5">
            <Loader2 size={16} className="text-violet-500 animate-spin" />
            <span className="text-sm font-medium text-slate-600">AI 正在规划 Mod...</span>
          </div>
          {batchLog.length > 0 && <AgentLog lines={batchLog} />}
          {socketRef.current && (
            <button
              onClick={cancelBatch}
              className="py-1.5 px-3 rounded-lg border border-red-200 text-red-600 hover:bg-red-50 text-sm transition-colors flex items-center gap-1.5"
            >
              <StopCircle size={13} />
              停止规划
            </button>
          )}
        </div>
      )}

      {/* 审阅 Item */}
      {stage === "review_items" && plan && (
        <ReviewPlan
          plan={plan}
          review={planReview}
          reviewStrictness={reviewStrictness}
          reviewBusy={reviewBusy}
          reviewError={reviewError}
          reviewFeedback={reviewFeedback}
          focusItemId={reviewFocusItemId}
          editedItems={editedItems}
          setEditedItems={updateEditedItems}
          onRefreshReview={() => {
            void refreshPlanReview();
          }}
          onStrictnessChange={handleReviewStrictnessChange}
          onConfirm={() => {
            void handleConfirmItemsReview();
          }}
          onReset={reset}
        />
      )}

      {/* 审阅执行策略 */}
      {stage === "review_bundles" && plan && (
        <ReviewBundles
          items={editedItems}
          review={planReview}
          reviewStrictness={reviewStrictness}
          reviewBusy={reviewBusy}
          reviewError={reviewError}
          reviewFeedback={reviewFeedback}
          bundleDecisions={bundleDecisions}
          onBack={() => dispatchRuntime({ type: "stage_set", stage: "review_items" })}
          onRefreshReview={() => {
            void refreshPlanReview();
          }}
          onStrictnessChange={handleReviewStrictnessChange}
          onBundleDecisionChange={handleBundleDecisionChange}
          onBundleSplitRequest={(bundleKey) => {
            void handleBundleSplitRequest(bundleKey);
          }}
          onBundleReturnToItems={handleBundleReturnToItems}
          onConfirm={() => {
            void handleConfirmBundleReview();
          }}
          onReset={reset}
        />
      )}

      {/* 全局错误 */}
      {stage === "error" && (
        <div className="rounded-xl border border-red-200 bg-red-50 p-5 space-y-3">
          <div className="flex items-start gap-2">
            <AlertTriangle size={16} className="text-red-500 shrink-0 mt-0.5" />
            <div>
              <p className="text-sm font-semibold text-red-700">规划失败</p>
              {workflowErrorMessage && (
                <pre className="text-xs text-red-600 font-mono mt-1 whitespace-pre-wrap">{workflowErrorMessage}</pre>
              )}
            </div>
          </div>
          <button onClick={reset} className="text-sm text-red-500 hover:text-red-700 flex items-center gap-1">
            <RotateCcw size={13} /> 重试
          </button>
        </div>
      )}

      {/* 执行阶段 + 完成 */}
      {(stage === "executing" || stage === "done" || stage === "cancelled") && (
        <ExecutionView
          items={editedItems}
          itemStates={itemStates}
          activeItemId={activeItemId}
          setActiveItemId={(itemId) => dispatchRuntime({ type: "active_item_set", itemId })}
          batchLog={batchLog}
          currentBatchStage={currentBatchStage}
          batchStageHistory={batchStageHistory}
          batchResult={batchResult}
          stage={stage}
          projectRoot={projectRoot}
          onStatusNotice={onStatusNotice}
          autoSelectFirst={autoSelectFirst}
          onAutoSelectToggle={() => setAutoSelectFirst((v) => !v)}
          onSelectImage={handleSelectImage}
          onGenerateMore={handleGenerateMore}
          onRetryItem={handleRetryItem}
          approvalBusyActionId={approvalBusyActionId}
          onApproveAction={(actionId) => {
            void handleApprovalAction(actionId, approveApproval);
          }}
          onRejectAction={(actionId) => {
            void handleApprovalAction(actionId, (id) => rejectApproval(id));
          }}
          onExecuteAction={(actionId) => {
            void handleApprovalAction(actionId, executeApproval);
          }}
          onProceedApproval={handleProceedApproval}
          hasLiveSession={socketRef.current !== null}
          onCancelWorkflow={cancelBatch}
          onUpdatePrompt={(id, prompt) => updateItem(id, { currentPrompt: prompt })}
          onToggleMorePrompt={(id) => updateItem(id, { showMorePrompt: !itemStates[id]?.showMorePrompt })}
          onReset={reset}
        />
      )}
    </div>
  );
}

export function BatchGenerationFeatureView({
  onRequestExecution,
  onStatusNotice,
  knowledgeStatus,
  onOpenKnowledgeGuide,
  onOpenSettings,
}: WorkspaceFeatureAdapterProps) {
  const resolvedProps = useResolvedWorkspaceFeatureProps({
    onRequestExecution,
    onStatusNotice,
    knowledgeStatus,
    onOpenKnowledgeGuide,
    onOpenSettings,
  });
  return (
    <BatchModePage
      onRequestExecution={resolvedProps.onRequestExecution}
      onStatusNotice={resolvedProps.onStatusNotice}
      knowledgeStatus={resolvedProps.knowledgeStatus}
      onOpenKnowledgeGuide={resolvedProps.onOpenKnowledgeGuide}
      onOpenSettings={resolvedProps.onOpenSettings}
    />
  );
}

export default function BatchMode() {
  return <BatchGenerationFeatureView />;
}
