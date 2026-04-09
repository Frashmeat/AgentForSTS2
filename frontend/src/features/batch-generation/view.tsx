import { useState, useCallback, useRef, useEffect } from "react";
import {
  Loader2, ChevronDown, ChevronUp, RotateCcw,
  CheckCircle2, XCircle, Clock, ImageIcon, Code2, Sparkles, AlertTriangle,
  Upload, Wand2,
} from "lucide-react";
import { ApprovalPanel } from "../../components/ApprovalPanel";
import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "../../lib/approvals";
import { BatchSocket, PlanItem, ModPlan } from "../../lib/batch_ws";
import { pickActiveItemOnDone, pickActiveItemOnStart } from "../../lib/batchActiveItem";
import { AgentLog } from "../../components/AgentLog";
import { StageStatus } from "../../components/StageStatus";
import { BuildDeploy } from "../../components/BuildDeploy";
import { ProjectRootField } from "../../components/ProjectRootField";
import { cn } from "../../lib/utils";
import { loadAppConfig } from "../../shared/api/config";
import { reviewModPlan } from "../../shared/api/workflow.ts";
import { resolveErrorMessage, resolveWorkflowErrorMessage } from "../../shared/error.ts";
import type {
  ExecutionBundlePreview,
  PlanItemValidation,
  PlanReviewPayload,
} from "../../shared/types/workflow.ts";
import {
  appendWorkflowLogEntry,
  resolveNextWorkflowModel,
  type WorkflowLogEntry,
} from "../../shared/workflowLog.ts";
import type { PlatformExecutionRequest } from "../platform-run/types.ts";
import {
  applyBatchApprovalUpdate,
  canProceedBatchApproval,
  markBatchApprovalResuming,
  resumeBatchApprovalWorkflow,
} from "./approval";
import { openBatchPlanningSocket } from "./planningSession";
import { canProceedFromBundleReview, type ReviewStrictness } from "./state.ts";

// ── 类型 ──────────────────────────────────────────────────────────────────────

type BatchStage = "input" | "planning" | "review_items" | "review_bundles" | "executing" | "done" | "error";

type ItemStatus =
  | "pending"
  | "img_generating"
  | "awaiting_selection"
  | "approval_pending"
  | "code_generating"
  | "done"
  | "error";

interface ItemState {
  status: ItemStatus;
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

function defaultItemState(): ItemState {
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

// ── 工具函数 ──────────────────────────────────────────────────────────────────

const TYPE_LABELS: Record<string, string> = {
  card: "卡牌",
  card_fullscreen: "全画面卡",
  relic: "遗物",
  power: "Power",
  character: "角色",
  custom_code: "代码",
};

const STATUS_ICONS: Record<ItemStatus, React.ReactNode> = {
  pending:            <Clock size={14} className="text-slate-300" />,
  img_generating:     <Loader2 size={14} className="text-violet-400 animate-spin" />,
  awaiting_selection: <ImageIcon size={14} className="text-violet-500" />,
  approval_pending:   <Clock size={14} className="text-violet-500" />,
  code_generating:    <Code2 size={14} className="text-blue-400 animate-pulse" />,
  done:               <CheckCircle2 size={14} className="text-green-500" />,
  error:              <XCircle size={14} className="text-red-500" />,
};

const STATUS_LABELS: Record<ItemStatus, string> = {
  pending:            "等待中",
  img_generating:     "生成图像",
  awaiting_selection: "等待选图",
  approval_pending:   "等待审批",
  code_generating:    "生成代码",
  done:               "完成",
  error:              "失败",
};

const PLAN_STORAGE_KEY = "ats_last_plan";
const PLAN_ITEMS_STORAGE_KEY = "ats_last_plan_items";
const PLAN_REVIEW_STORAGE_KEY = "ats_last_plan_review";
const PLAN_REVIEW_STRICTNESS_STORAGE_KEY = "ats_last_plan_review_strictness";

const REVIEW_STATUS_LABELS: Record<PlanItemValidation["status"], string> = {
  clear: "可继续",
  needs_user_input: "待补充",
  invalid: "存在错误",
};

const BUNDLE_STATUS_LABELS: Record<ExecutionBundlePreview["status"], string> = {
  clear: "可执行",
  needs_confirmation: "需确认",
  split_recommended: "建议拆分",
};

const STRICTNESS_OPTIONS: Array<{ value: ReviewStrictness; label: string; description: string }> = [
  { value: "efficient", label: "高效率", description: "减少拦截，尽快进入执行" },
  { value: "balanced", label: "平衡", description: "兼顾确认成本和执行安全" },
  { value: "strict", label: "严格", description: "更细地检查描述和分组风险" },
];

function normalizeReviewStrictness(value: unknown): ReviewStrictness {
  return value === "efficient" || value === "strict" ? value : "balanced";
}

function readJsonStorage<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) as T : fallback;
  } catch {
    return fallback;
  }
}

function writeJsonStorage(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {}
}

function writeTextStorage(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {}
}

function resolvePlanFieldValue(item: PlanItem, field: string): unknown {
  return (item as unknown as Record<string, unknown>)[field];
}

function hasMeaningfulPlanFieldValue(item: PlanItem, field: string): boolean {
  const value = resolvePlanFieldValue(item, field);
  if (Array.isArray(value)) {
    return value.some((entry) => typeof entry === "string" && entry.trim().length > 0);
  }
  if (typeof value === "string") {
    return value.trim().length > 0;
  }
  if (typeof value === "boolean") {
    return value;
  }
  return value !== null && value !== undefined;
}

function canProceedFromEditedItemReview(
  review: PlanReviewPayload | null,
  items: PlanItem[],
): boolean {
  if (!review) {
    return true;
  }

  return review.validation.items.every((reviewItem) => {
    if (reviewItem.status === "clear") {
      return true;
    }
    if (reviewItem.status === "invalid") {
      return false;
    }
    const item = items.find((candidate) => candidate.id === reviewItem.item_id);
    if (!item) {
      return false;
    }
    if (reviewItem.missing_fields.some((field) => !hasMeaningfulPlanFieldValue(item, field))) {
      return false;
    }
    return reviewItem.issues.every((issue) => !issue.field || hasMeaningfulPlanFieldValue(item, issue.field));
  });
}

// ── 主组件 ────────────────────────────────────────────────────────────────────

function BatchModePage({ onRequestExecution }: { onRequestExecution?: (request: PlatformExecutionRequest) => void }) {
  const [stage, setStage] = useState<BatchStage>("input");
  const [requirements, setRequirements] = useState("");
  const [projectRoot, setProjectRoot] = useState("");

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
  const [planReview, setPlanReview] = useState<PlanReviewPayload | null>(() =>
    readJsonStorage<PlanReviewPayload | null>(PLAN_REVIEW_STORAGE_KEY, null),
  );
  const [reviewStrictness, setReviewStrictness] = useState<ReviewStrictness>(() => {
    try {
      return normalizeReviewStrictness(localStorage.getItem(PLAN_REVIEW_STRICTNESS_STORAGE_KEY));
    } catch {
      return "balanced";
    }
  });
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  const [activeItemId, setActiveItemId] = useState<string | null>(null);
  const [itemStates, setItemStates] = useState<Record<string, ItemState>>({});
  const itemStatesRef = useRef<Record<string, ItemState>>({});
  const [batchLog, setBatchLog] = useState<string[]>([]);
  const [currentBatchStage, setCurrentBatchStage] = useState<string | null>(null);
  const [batchStageHistory, setBatchStageHistory] = useState<string[]>([]);
  const [globalError, setGlobalError] = useState<string | null>(null);
  const [batchResult, setBatchResult] = useState<{ success: number; error: number } | null>(null);
  const [approvalBusyActionId, setApprovalBusyActionId] = useState<string | null>(null);

  const [autoSelectFirst, setAutoSelectFirst] = useState(false);
  const autoSelectRef = useRef(false);
  useEffect(() => { autoSelectRef.current = autoSelectFirst; }, [autoSelectFirst]);
  const socketRef = useRef<BatchSocket | null>(null);

  // ── State updater helpers ─────────────────────────────────────────────────

  const applyItemStates = useCallback((updater: (prev: Record<string, ItemState>) => Record<string, ItemState>) => {
    const next = updater(itemStatesRef.current);
    itemStatesRef.current = next;
    setItemStates(next);
  }, []);

  const updateItem = useCallback((id: string, patch: Partial<ItemState>) => {
    applyItemStates(prev => ({
      ...prev,
      [id]: { ...(prev[id] ?? defaultItemState()), ...patch },
    }));
  }, [applyItemStates]);

  const appendProgress = useCallback((id: string, msg: string) => {
    applyItemStates(prev => {
      const cur = prev[id] ?? defaultItemState();
      return { ...prev, [id]: { ...cur, progress: [...cur.progress, msg] } };
    });
  }, [applyItemStates]);

  const appendAgent = useCallback((id: string, entry: WorkflowLogEntry) => {
    applyItemStates(prev => {
      const cur = prev[id] ?? defaultItemState();
      return {
        ...prev,
        [id]: {
          ...cur,
          agentLog: [...cur.agentLog, entry.text],
          agentLogEntries: appendWorkflowLogEntry(cur.agentLogEntries, entry),
          currentAgentModel: resolveNextWorkflowModel(cur.currentAgentModel, entry),
        },
      };
    });
  }, [applyItemStates]);

  const addImage = useCallback((id: string, b64: string, index: number, prompt: string) => {
    applyItemStates(prev => {
      const cur = prev[id] ?? defaultItemState();
      const images = [...cur.images];
      images[index] = b64;
      return { ...prev, [id]: { ...cur, images, currentPrompt: prompt, status: "awaiting_selection" } };
    });
    // 如果当前没有 active item，自动跳到这个需要选图的 item
    setActiveItemId(prev => prev ?? id);
  }, [applyItemStates]);

  // ── Start ─────────────────────────────────────────────────────────────────

  async function startPlanning() {
    if (!requirements.trim()) return;
    setStage("planning");
    setBatchLog([]);
    setGlobalError(null);
    setReviewError(null);
    setPlanReview(null);
    itemStatesRef.current = {};
    setItemStates({});
    setBatchResult(null);
    setCurrentBatchStage(null);
    setBatchStageHistory([]);
    setPlan(null);
    setActiveItemId(null);

    const ws = new BatchSocket();
    socketRef.current = ws;
    _registerBatchHandlers(ws);
    const started = await openBatchPlanningSocket(ws, {
      requirements,
      projectRoot,
      onOpenError(message) {
        socketRef.current = null;
        setGlobalError(message);
        setStage("error");
      },
    });
    if (!started) {
      return;
    }
  }

  function _registerBatchHandlers(ws: BatchSocket) {
    ws.on("planning", () => setBatchLog(l => [...l, "正在规划 Mod..."]));
    ws.on("plan_ready", (d) => {
      setPlan(d.plan);
      setEditedItems(d.plan.items);
      setPlanReview(d.review ?? null);
      setReviewStrictness((current) => normalizeReviewStrictness(d.review?.strictness ?? current));
      setReviewError(null);
      setStage("review_items");
      try {
        writeJsonStorage(PLAN_STORAGE_KEY, d.plan);
        writeJsonStorage(PLAN_ITEMS_STORAGE_KEY, d.plan.items);
        writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, d.review ?? null);
        writeTextStorage(
          PLAN_REVIEW_STRICTNESS_STORAGE_KEY,
          normalizeReviewStrictness(d.review?.strictness ?? reviewStrictness),
        );
      } catch {}
    });
    ws.on("batch_progress", (d) => setBatchLog(l => [...l, d.message]));
    ws.on("stage_update", (d) => {
      if (d.item_id) {
        applyItemStates(prev => {
          const cur = prev[d.item_id!] ?? defaultItemState();
          return {
            ...prev,
            [d.item_id!]: {
              ...cur,
              currentStage: d.message,
              stageHistory: cur.stageHistory[cur.stageHistory.length - 1] === d.message ? cur.stageHistory : [...cur.stageHistory, d.message],
            },
          };
        });
        return;
      }
      setCurrentBatchStage(d.message);
      setBatchStageHistory(prev => prev[prev.length - 1] === d.message ? prev : [...prev, d.message]);
    });
    ws.on("batch_started", (d) => {
      const init: Record<string, ItemState> = {};
      d.items.forEach(it => { init[it.id] = defaultItemState(); });
      itemStatesRef.current = init;
      setItemStates(init);
      setStage("executing");
      setActiveItemId(d.items[0]?.id ?? null);
    });
    ws.on("item_started", (d) => {
      updateItem(d.item_id, { status: "img_generating" });
      setActiveItemId(prev => pickActiveItemOnStart(prev, itemStatesRef.current, d.item_id));
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
      setActiveItemId(prev => prev ?? d.item_id);
    });
    ws.on("item_done", (d) => {
      updateItem(d.item_id, { status: "done" });
      setActiveItemId(prev => pickActiveItemOnDone(prev, d.item_id, itemStatesRef.current));
    });
    ws.on("item_error", (d) => {
      updateItem(d.item_id, {
        status: "error",
        error: resolveWorkflowErrorMessage(d),
        errorTrace: d.traceback ?? null,
      });
    });
    ws.on("batch_done", (d) => {
      setBatchResult({ success: d.success_count, error: d.error_count });
      setStage("done");
    });
    ws.on("error", (d) => { setGlobalError(resolveWorkflowErrorMessage(d)); setStage("error"); });
  }

  async function confirmPlan() {
    if (!plan) return;
    const itemsForStorage = editedItems.map(it => ({ ...it, provided_image_b64: undefined }));
    writeJsonStorage(PLAN_ITEMS_STORAGE_KEY, itemsForStorage);
    writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, planReview);
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
        },
        onOpenError(message) {
          socketRef.current = null;
          setGlobalError(message);
          setStage("error");
        },
      });
      if (!started) {
        return;
      }
      setStage("executing");
    } else {
      setStage("executing");
      socketRef.current.send({
        action: "confirm_plan",
        plan: { ...plan, items: editedItems },
        review_strictness: reviewStrictness,
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
  ): Promise<PlanReviewPayload | null> {
    if (!plan) {
      return null;
    }
    setReviewBusy(true);
    setReviewError(null);
    try {
      const review = await reviewModPlan({
        plan: { ...plan, items },
        strictness,
      });
      setPlanReview(review);
      setReviewStrictness(normalizeReviewStrictness(review.strictness));
      writeJsonStorage(PLAN_REVIEW_STORAGE_KEY, review);
      writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, normalizeReviewStrictness(review.strictness));
      return review;
    } catch (error) {
      setReviewError(resolveErrorMessage(error));
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
      setStage("review_bundles");
    }
  }

  async function handleConfirmBundleReview() {
    const review = await refreshPlanReview();
    if (!review) {
      return;
    }
    if (!canProceedFromEditedItemReview(review, editedItems)) {
      setStage("review_items");
      return;
    }
    if (!canProceedFromBundleReview(review)) {
      setStage("review_bundles");
      return;
    }
    requestExecutionStart();
  }

  function handleReviewStrictnessChange(nextStrictness: ReviewStrictness) {
    setReviewStrictness(nextStrictness);
    writeTextStorage(PLAN_REVIEW_STRICTNESS_STORAGE_KEY, nextStrictness);
    if (plan) {
      void refreshPlanReview(editedItems, nextStrictness);
    }
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
      requiresImageAi: editedItems.some(item => item.needs_image && !item.provided_image_b64),
      items: editedItems.map((item, index) => ({
        item_type: item.type,
        input_summary: item.description || item.name,
        input_payload: {
          item_index: index,
          name: item.name,
          description: item.description,
          goal: item.goal,
          detailed_description: item.detailed_description,
          scope_boundary: item.scope_boundary,
          dependency_reason: item.dependency_reason,
          acceptance_notes: item.acceptance_notes,
          affected_targets: item.affected_targets,
          coupling_kind: item.coupling_kind,
          needs_image: item.needs_image,
          has_uploaded_image: Boolean(item.provided_image_b64),
          image_description: item.image_description,
          implementation_notes: item.implementation_notes,
          depends_on: item.depends_on,
        },
      })),
      runLocal: executeLocal,
    });
  }

  function handleSelectImage(itemId: string, index: number) {
    if (!socketRef.current) return;
    socketRef.current.send({ action: "select_image", item_id: itemId, index });
    updateItem(itemId, { status: "code_generating" });
    const nextAwaiting = editedItems.find(
      it => it.id !== itemId && itemStatesRef.current[it.id]?.status === "awaiting_selection"
    );
    if (nextAwaiting) setActiveItemId(nextAwaiting.id);
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

  function reset() {
    socketRef.current?.close();
    socketRef.current = null;
    setStage("input");
    setPlan(null);
    setEditedItems([]);
    setPlanReview(null);
    setReviewError(null);
    itemStatesRef.current = {};
    setItemStates({});
    setBatchLog([]);
    setGlobalError(null);
    setBatchResult(null);
    setActiveItemId(null);
    setApprovalBusyActionId(null);
  }

  async function handleApprovalAction(
    actionId: string,
    action: (id: string) => Promise<ApprovalRequest>,
  ) {
    setApprovalBusyActionId(actionId);
    try {
      const updated = await action(actionId);
      applyItemStates(prev => applyBatchApprovalUpdate(prev, actionId, updated));
    } catch (error) {
      setGlobalError(resolveErrorMessage(error));
      setStage("error");
    } finally {
      setApprovalBusyActionId(null);
    }
  }

  function handleProceedApproval(itemId: string) {
    const socket = socketRef.current;
    const state = itemStatesRef.current[itemId];
    if (!socket || !state || !canProceedBatchApproval(state.approvalRequests)) {
      return;
    }

    applyItemStates(prev => markBatchApprovalResuming(prev, itemId));
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
            onChange={e => setRequirements(e.target.value)}
            rows={6}
            placeholder={"例如：\n我想做一个暗法师角色，主题是腐化和献祭。\n包含3张卡牌（攻击、技能、力量各一张），\n2个遗物（战斗开始触发），\n以及一个腐化叠层的 buff 机制。"}
            className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-violet-400 focus:ring-1 focus:ring-violet-100 resize-none"
          />
          <ProjectRootField
            value={projectRoot}
            placeholder="E:/STS2mod"
            showCreateAction={false}
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
              onClick={() => setStage("review_items")}
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
          editedItems={editedItems}
          setEditedItems={updateEditedItems}
          onRefreshReview={() => { void refreshPlanReview(); }}
          onStrictnessChange={handleReviewStrictnessChange}
          onConfirm={() => { void handleConfirmItemsReview(); }}
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
          onBack={() => setStage("review_items")}
          onRefreshReview={() => { void refreshPlanReview(); }}
          onStrictnessChange={handleReviewStrictnessChange}
          onConfirm={() => { void handleConfirmBundleReview(); }}
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
              {globalError && <pre className="text-xs text-red-600 font-mono mt-1 whitespace-pre-wrap">{globalError}</pre>}
            </div>
          </div>
          <button onClick={reset} className="text-sm text-red-500 hover:text-red-700 flex items-center gap-1">
            <RotateCcw size={13} /> 重试
          </button>
        </div>
      )}

      {/* 执行阶段 + 完成 */}
      {(stage === "executing" || stage === "done") && (
        <ExecutionView
          items={editedItems}
          itemStates={itemStates}
          activeItemId={activeItemId}
          setActiveItemId={setActiveItemId}
          batchLog={batchLog}
          currentBatchStage={currentBatchStage}
          batchStageHistory={batchStageHistory}
          batchResult={batchResult}
          stage={stage}
          projectRoot={projectRoot}
          autoSelectFirst={autoSelectFirst}
          onAutoSelectToggle={() => setAutoSelectFirst(v => !v)}
          onSelectImage={handleSelectImage}
          onGenerateMore={handleGenerateMore}
          onRetryItem={handleRetryItem}
          approvalBusyActionId={approvalBusyActionId}
          onApproveAction={(actionId) => { void handleApprovalAction(actionId, approveApproval); }}
          onRejectAction={(actionId) => { void handleApprovalAction(actionId, (id) => rejectApproval(id)); }}
          onExecuteAction={(actionId) => { void handleApprovalAction(actionId, executeApproval); }}
          onProceedApproval={handleProceedApproval}
          hasLiveSession={socketRef.current !== null}
          onUpdatePrompt={(id, prompt) =>
            updateItem(id, { currentPrompt: prompt })
          }
          onToggleMorePrompt={(id) =>
            applyItemStates(prev => ({
              ...prev,
              [id]: { ...prev[id], showMorePrompt: !prev[id]?.showMorePrompt },
            }))
          }
          onReset={reset}
        />
      )}
    </div>
  );
}

export function BatchGenerationFeatureView({
  onRequestExecution,
}: {
  onRequestExecution?: (request: PlatformExecutionRequest) => void;
}) {
  return <BatchModePage onRequestExecution={onRequestExecution} />;
}

export default function BatchMode() {
  return <BatchGenerationFeatureView />;
}

// ── 计划审阅组件 ──────────────────────────────────────────────────────────────

function ReviewStatusBadge({
  status,
  kind,
}: {
  status: PlanItemValidation["status"] | ExecutionBundlePreview["status"];
  kind: "item" | "bundle";
}) {
  const label = kind === "item"
    ? REVIEW_STATUS_LABELS[status as PlanItemValidation["status"]]
    : BUNDLE_STATUS_LABELS[status as ExecutionBundlePreview["status"]];
  const tone = status === "clear"
    ? "bg-green-50 text-green-700 border-green-200"
    : status === "invalid"
      ? "bg-red-50 text-red-700 border-red-200"
      : "bg-amber-50 text-amber-700 border-amber-200";
  return <span className={cn("text-xs rounded-full border px-2 py-0.5 font-medium", tone)}>{label}</span>;
}

function ReviewStrictnessSelector({
  value,
  disabled,
  onChange,
}: {
  value: ReviewStrictness;
  disabled: boolean;
  onChange: (value: ReviewStrictness) => void;
}) {
  return (
    <div className="space-y-2">
      <div>
        <p className="text-sm font-medium text-slate-700">判断严格度</p>
        <p className="text-xs text-slate-400">控制补充说明的严格程度，以及 bundle 拆分时的谨慎程度。</p>
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        {STRICTNESS_OPTIONS.map((option) => (
          <button
            key={option.value}
            type="button"
            disabled={disabled}
            onClick={() => onChange(option.value)}
            className={cn(
              "rounded-xl border px-3 py-2 text-left transition-colors disabled:opacity-60",
              value === option.value
                ? "border-violet-300 bg-violet-50"
                : "border-slate-200 bg-white hover:border-violet-200 hover:bg-violet-50/60",
            )}
          >
            <p className="text-sm font-semibold text-slate-800">{option.label}</p>
            <p className="mt-1 text-xs text-slate-500">{option.description}</p>
          </button>
        ))}
      </div>
    </div>
  );
}

function ReviewNotice({ message }: { message: string | null }) {
  if (!message) {
    return null;
  }
  return (
    <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">
      {message}
    </div>
  );
}

function ReviewPlan({
  plan,
  review,
  reviewStrictness,
  reviewBusy,
  reviewError,
  editedItems,
  setEditedItems,
  onRefreshReview,
  onStrictnessChange,
  onConfirm,
  onReset,
}: {
  plan: ModPlan;
  review: PlanReviewPayload | null;
  reviewStrictness: ReviewStrictness;
  reviewBusy: boolean;
  reviewError: string | null;
  editedItems: PlanItem[];
  setEditedItems: (items: PlanItem[]) => void;
  onRefreshReview: () => void;
  onStrictnessChange: (value: ReviewStrictness) => void;
  onConfirm: () => void;
  onReset: () => void;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [uploadPreviews, setUploadPreviews] = useState<Record<string, string>>({});
  const validationById = new Map((review?.validation.items ?? []).map((item) => [item.item_id, item]));
  const clearCount = review?.validation.items.filter((item) => item.status === "clear").length ?? 0;
  const canProceed = canProceedFromEditedItemReview(review, editedItems);

  function updateItem(id: string, patch: Partial<PlanItem>) {
    setEditedItems(editedItems.map(it => it.id === id ? { ...it, ...patch } : it));
  }

  function updateStringList(id: string, field: "depends_on" | "affected_targets", value: string) {
    updateItem(id, {
      [field]: value
        .split(/[\n,]/)
        .map((entry) => entry.trim())
        .filter(Boolean),
    } as Partial<PlanItem>);
  }

  function handleImageFile(id: string, file: File) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const dataUrl = e.target?.result as string;
      const b64 = dataUrl.split(",")[1];
      setUploadPreviews(p => ({ ...p, [id]: dataUrl }));
      updateItem(id, { provided_image_b64: b64 });
    };
    reader.readAsDataURL(file);
  }

  return (
    <div className="space-y-4">
      <div className="workspace-surface rounded-2xl p-5">
        <div className="flex flex-col gap-4 mb-4">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="text-xs uppercase tracking-[0.2em] text-violet-500">Step 1 / 2</p>
              <h2 className="font-bold text-slate-800">{plan.mod_name}</h2>
              <p className="text-xs text-slate-500 mt-0.5">{plan.summary}</p>
            </div>
            <span className="text-xs text-violet-700 bg-violet-50 border border-violet-200 rounded-full px-2 py-0.5 font-medium">
              {clearCount}/{editedItems.length} 项可进入下一步
            </span>
          </div>

          <ReviewStrictnessSelector
            value={reviewStrictness}
            disabled={reviewBusy}
            onChange={onStrictnessChange}
          />

          <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-sm font-medium text-slate-700">当前阶段：逐项确认计划描述</p>
            <p className="mt-1 text-xs text-slate-500">
              先把每个 item 的目标、范围、依赖原因和验收说明确认清楚，再进入执行策略分组确认。
            </p>
            {!canProceed && (
              <p className="mt-2 text-xs font-medium text-amber-700">
                仍有 item 需要补充说明。修改字段后，点击下方按钮会重新检查当前计划。
              </p>
            )}
          </div>
        </div>

        <ReviewNotice message={reviewError} />

        <div className="space-y-2">
          {editedItems.map(item => {
            const validation = validationById.get(item.id);
            const missingFields = validation?.missing_fields ?? [];
            const issues = validation?.issues ?? [];
            const questions = validation?.clarification_questions ?? [];

            return (
            <div key={item.id} className="rounded-lg border border-slate-200 bg-slate-50 overflow-hidden">
              <button
                type="button"
                className="w-full flex items-center gap-3 px-3 py-2.5 text-left hover:bg-slate-100 transition-colors"
                onClick={() => setExpandedId(expandedId === item.id ? null : item.id)}
              >
                <span className="text-xs font-medium text-slate-400 bg-slate-200 rounded px-1.5 py-0.5 shrink-0">
                  {TYPE_LABELS[item.type] ?? item.type}
                </span>
                <span className="text-sm font-medium text-slate-700 flex-1">{item.name}</span>
                {validation && <ReviewStatusBadge status={validation.status} kind="item" />}
                {item.depends_on.length > 0 && (
                  <span className="text-xs text-slate-400">依赖 {item.depends_on.length}</span>
                )}
                {expandedId === item.id
                  ? <ChevronUp size={13} className="text-slate-400 shrink-0" />
                  : <ChevronDown size={13} className="text-slate-400 shrink-0" />
                }
              </button>

              {expandedId === item.id && (
                <div className="px-3 pb-3 space-y-3 border-t border-slate-200 pt-2.5">
                  {validation && (
                    <div className="rounded-xl border border-slate-200 bg-white px-3 py-3 space-y-2">
                      <div className="flex items-center justify-between gap-3">
                        <p className="text-sm font-semibold text-slate-700">当前评审结果</p>
                        <ReviewStatusBadge status={validation.status} kind="item" />
                      </div>
                      {issues.length > 0 && (
                        <div className="flex flex-wrap gap-2">
                          {issues.map((issue) => (
                            <span key={`${issue.code}-${issue.field ?? "base"}`} className="rounded-full bg-red-50 px-2 py-0.5 text-xs text-red-700">
                              {issue.message}
                            </span>
                          ))}
                        </div>
                      )}
                      {missingFields.length > 0 && (
                        <div className="flex flex-wrap gap-2">
                          {missingFields.map((field) => (
                            <span key={field} className="rounded-full bg-amber-50 px-2 py-0.5 text-xs text-amber-700">
                              待补：{field}
                            </span>
                          ))}
                        </div>
                      )}
                      {questions.length > 0 && (
                        <div className="space-y-1">
                          {questions.map((question, index) => (
                            <p key={`${item.id}-question-${index}`} className="text-xs text-slate-600">
                              {index + 1}. {question}
                            </p>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                  <div className="space-y-1">
                    <label className="text-xs text-slate-400">名称（英文）</label>
                    <input
                      value={item.name}
                      onChange={e => updateItem(item.id, { name: e.target.value })}
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm focus:outline-none focus:border-violet-400"
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs text-slate-400">目标</label>
                    <input
                      value={item.goal}
                      onChange={e => updateItem(item.id, { goal: e.target.value })}
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm focus:outline-none focus:border-violet-400"
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs text-slate-400">详细描述</label>
                    <textarea
                      value={item.detailed_description}
                      onChange={e => updateItem(item.id, { detailed_description: e.target.value })}
                      rows={3}
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs text-slate-400">用户描述摘要</label>
                    <textarea
                      value={item.description}
                      onChange={e => updateItem(item.id, { description: e.target.value })}
                      rows={2}
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                    />
                  </div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">范围边界</label>
                      <textarea
                        value={item.scope_boundary}
                        onChange={e => updateItem(item.id, { scope_boundary: e.target.value })}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">依赖原因</label>
                      <textarea
                        value={item.dependency_reason}
                        onChange={e => updateItem(item.id, { dependency_reason: e.target.value })}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>
                  </div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">验收说明</label>
                      <textarea
                        value={item.acceptance_notes}
                        onChange={e => updateItem(item.id, { acceptance_notes: e.target.value })}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">耦合类型</label>
                      <select
                        value={item.coupling_kind}
                        onChange={e => updateItem(item.id, { coupling_kind: e.target.value })}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-2 text-sm focus:outline-none focus:border-violet-400"
                      >
                        <option value="unclear">unclear</option>
                        <option value="order_only">order_only</option>
                        <option value="feature_bundle">feature_bundle</option>
                        <option value="shared_logic">shared_logic</option>
                        <option value="isolated">isolated</option>
                      </select>
                    </div>
                  </div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">依赖项（逗号或换行分隔）</label>
                      <textarea
                        value={item.depends_on.join("\n")}
                        onChange={e => updateStringList(item.id, "depends_on", e.target.value)}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-slate-400">影响目标（逗号或换行分隔）</label>
                      <textarea
                        value={item.affected_targets.join("\n")}
                        onChange={e => updateStringList(item.id, "affected_targets", e.target.value)}
                        rows={3}
                        className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                      />
                    </div>
                  </div>
                  {item.needs_image && (
                    <div className="space-y-2">
                      {/* 图片模式切换 */}
                      <div className="flex items-center gap-1.5">
                        <span className="text-xs text-slate-400">图片来源：</span>
                        <button
                          onClick={() => updateItem(item.id, { provided_image_b64: undefined })}
                          className={cn(
                            "flex items-center gap-1 px-2.5 py-1 rounded text-xs font-medium transition-colors",
                            !item.provided_image_b64
                              ? "bg-violet-700 text-white"
                              : "bg-slate-100 text-slate-500 hover:bg-slate-200"
                          )}
                        >
                          <Wand2 size={11} /> AI 生成
                        </button>
                        <button
                          onClick={() => {
                            const input = document.createElement("input");
                            input.type = "file"; input.accept = "image/*";
                            input.onchange = () => { if (input.files?.[0]) handleImageFile(item.id, input.files[0]); };
                            input.click();
                          }}
                          className={cn(
                            "flex items-center gap-1 px-2.5 py-1 rounded text-xs font-medium transition-colors",
                            item.provided_image_b64
                              ? "bg-violet-700 text-white"
                              : "bg-slate-100 text-slate-500 hover:bg-slate-200"
                          )}
                        >
                          <Upload size={11} /> 上传图片
                        </button>
                      </div>
                      {/* 上传预览 */}
                      {item.provided_image_b64 && uploadPreviews[item.id] && (
                        <div className="relative w-24 h-24 rounded-lg overflow-hidden border border-violet-300">
                          <img src={uploadPreviews[item.id]} alt="preview" className="w-full h-full object-cover" />
                          <button
                            onClick={() => {
                              updateItem(item.id, { provided_image_b64: undefined });
                              setUploadPreviews(p => { const n = { ...p }; delete n[item.id]; return n; });
                            }}
                            className="absolute top-0.5 right-0.5 w-4 h-4 rounded-full bg-black/60 text-white text-xs flex items-center justify-center hover:bg-red-500"
                          >×</button>
                        </div>
                      )}
                      {/* AI 生成时显示图像描述 */}
                      {!item.provided_image_b64 && (
                        <div className="space-y-1">
                          <label className="text-xs text-slate-400">图像描述（AI 生图用）</label>
                          <textarea
                            value={item.image_description}
                            onChange={e => updateItem(item.id, { image_description: e.target.value })}
                            rows={2}
                            className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-violet-400"
                          />
                        </div>
                      )}
                    </div>
                  )}
                  <div className="space-y-1">
                    <label className="text-xs text-slate-400">技术实现说明（给 Code Agent）</label>
                    <textarea
                      value={item.implementation_notes}
                      onChange={e => updateItem(item.id, { implementation_notes: e.target.value })}
                      rows={3}
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-xs font-mono resize-none focus:outline-none focus:border-violet-400"
                    />
                  </div>
                </div>
              )}
            </div>
            );
          })}
        </div>

        <div className="flex flex-wrap gap-2 mt-4">
          <button
            type="button"
            onClick={onRefreshReview}
            disabled={reviewBusy}
            className="py-2.5 px-4 rounded-lg border border-violet-200 text-violet-700 text-sm hover:bg-violet-50 transition-colors disabled:opacity-60"
          >
            {reviewBusy ? "重新检查中..." : "重新检查当前计划"}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={reviewBusy}
            className="flex-1 py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 transition-colors disabled:opacity-60"
          >
            {canProceed ? "进入执行策略确认" : "保存说明并重新检查"}
          </button>
          <button
            type="button"
            onClick={onReset}
            className="py-2.5 px-4 rounded-lg border border-slate-200 text-slate-400 hover:text-slate-600 text-sm transition-colors"
          >
            重来
          </button>
        </div>
      </div>
    </div>
  );
}

function ReviewBundles({
  items,
  review,
  reviewStrictness,
  reviewBusy,
  reviewError,
  onBack,
  onRefreshReview,
  onStrictnessChange,
  onConfirm,
  onReset,
}: {
  items: PlanItem[];
  review: PlanReviewPayload | null;
  reviewStrictness: ReviewStrictness;
  reviewBusy: boolean;
  reviewError: string | null;
  onBack: () => void;
  onRefreshReview: () => void;
  onStrictnessChange: (value: ReviewStrictness) => void;
  onConfirm: () => void;
  onReset: () => void;
}) {
  const itemNameMap = new Map(items.map((item) => [item.id, item.name]));
  const dependencyGroups = review?.execution_plan.dependency_groups ?? [];
  const executionBundles = review?.execution_plan.execution_bundles ?? [];
  const clearBundles = executionBundles.filter((bundle) => bundle.status === "clear").length;

  return (
    <div className="space-y-4">
      <div className="workspace-surface rounded-2xl p-5 space-y-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="text-xs uppercase tracking-[0.2em] text-violet-500">Step 2 / 2</p>
            <h2 className="font-bold text-slate-800">执行策略确认</h2>
            <p className="text-xs text-slate-500 mt-0.5">确认 item 如何分组执行，再进入真正的代码生成阶段。</p>
          </div>
          <span className="text-xs text-violet-700 bg-violet-50 border border-violet-200 rounded-full px-2 py-0.5 font-medium">
            {clearBundles}/{executionBundles.length || 0} 个 bundle 可直接执行
          </span>
        </div>

        <ReviewStrictnessSelector
          value={reviewStrictness}
          disabled={reviewBusy}
          onChange={onStrictnessChange}
        />

        <ReviewNotice message={reviewError} />

        <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
          <p className="text-sm font-medium text-slate-700">依赖分组预览</p>
          <div className="mt-2 flex flex-wrap gap-2">
            {dependencyGroups.length === 0 && (
              <span className="text-xs text-slate-400">暂无分组数据，先重新检查一次。</span>
            )}
            {dependencyGroups.map((group, index) => (
              <span key={`group-${index}`} className="rounded-full border border-slate-200 bg-white px-3 py-1 text-xs text-slate-600">
                G{index + 1}: {group.item_ids.map((itemId) => itemNameMap.get(itemId) ?? itemId).join(" / ")}
              </span>
            ))}
          </div>
        </div>

        <div className="space-y-3">
          {executionBundles.length === 0 && (
            <div className="rounded-xl border border-dashed border-slate-200 bg-slate-50 px-4 py-6 text-center text-sm text-slate-400">
              还没有 bundle 评审结果，点击“重新检查当前计划”即可生成。
            </div>
          )}
          {executionBundles.map((bundle, index) => (
            <div key={`bundle-${index}`} className="rounded-xl border border-slate-200 bg-white px-4 py-4 space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold text-slate-800">执行 Bundle {index + 1}</p>
                  <p className="mt-1 text-xs text-slate-500">
                    {bundle.item_ids.map((itemId) => itemNameMap.get(itemId) ?? itemId).join(" / ")}
                  </p>
                </div>
                <ReviewStatusBadge status={bundle.status} kind="bundle" />
              </div>
              <div className="grid gap-3 md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                <div className="rounded-lg bg-slate-50 px-3 py-3">
                  <p className="text-xs font-medium text-slate-500">分组理由</p>
                  <p className="mt-1 text-sm text-slate-700">{bundle.reason}</p>
                </div>
                <div className="rounded-lg bg-slate-50 px-3 py-3">
                  <p className="text-xs font-medium text-slate-500">风险标记</p>
                  <div className="mt-1 flex flex-wrap gap-2">
                    {bundle.risk_codes.length === 0 && (
                      <span className="text-sm text-green-700">无</span>
                    )}
                    {bundle.risk_codes.map((riskCode) => (
                      <span key={riskCode} className="rounded-full bg-amber-50 px-2 py-0.5 text-xs text-amber-700">
                        {riskCode}
                      </span>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>

        <div className="flex flex-wrap gap-2 mt-4">
          <button
            type="button"
            onClick={onBack}
            className="py-2.5 px-4 rounded-lg border border-slate-200 text-slate-600 hover:text-slate-800 text-sm transition-colors"
          >
            返回 Item 复核
          </button>
          <button
            type="button"
            onClick={onRefreshReview}
            disabled={reviewBusy}
            className="py-2.5 px-4 rounded-lg border border-violet-200 text-violet-700 text-sm hover:bg-violet-50 transition-colors disabled:opacity-60"
          >
            {reviewBusy ? "重新检查中..." : "重新检查当前计划"}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={reviewBusy}
            className="flex-1 py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 transition-colors disabled:opacity-60"
          >
            {canProceedFromBundleReview(review) ? "确认执行策略，开始执行" : "先处理 bundle 风险后再执行"}
          </button>
          <button
            type="button"
            onClick={onReset}
            className="py-2.5 px-4 rounded-lg border border-slate-200 text-slate-400 hover:text-slate-600 text-sm transition-colors"
          >
            重来
          </button>
        </div>
      </div>
    </div>
  );
}

// ── 执行视图 ──────────────────────────────────────────────────────────────────

function ExecutionView({
  items, itemStates, activeItemId, setActiveItemId,
  batchLog, currentBatchStage, batchStageHistory, batchResult, stage, projectRoot,
  autoSelectFirst, onAutoSelectToggle,
  onSelectImage, onGenerateMore, onRetryItem, approvalBusyActionId, onApproveAction, onRejectAction, onExecuteAction,
  onProceedApproval, hasLiveSession, onUpdatePrompt, onToggleMorePrompt, onReset,
}: {
  items: PlanItem[];
  itemStates: Record<string, ItemState>;
  activeItemId: string | null;
  setActiveItemId: (id: string) => void;
  batchLog: string[];
  currentBatchStage: string | null;
  batchStageHistory: string[];
  batchResult: { success: number; error: number } | null;
  stage: "executing" | "done";
  projectRoot: string;
  autoSelectFirst: boolean;
  onAutoSelectToggle: () => void;
  onSelectImage: (id: string, idx: number) => void;
  onGenerateMore: (id: string) => void;
  onRetryItem: (id: string) => void;
  approvalBusyActionId: string | null;
  onApproveAction: (actionId: string) => void;
  onRejectAction: (actionId: string) => void;
  onExecuteAction: (actionId: string) => void;
  onProceedApproval: (itemId: string) => void;
  hasLiveSession: boolean;
  onUpdatePrompt: (id: string, prompt: string) => void;
  onToggleMorePrompt: (id: string) => void;
  onReset: () => void;
}) {
  const awaitingCount = items.filter(it => itemStates[it.id]?.status === "awaiting_selection").length;
  const approvalCount = items.filter(it => itemStates[it.id]?.status === "approval_pending").length;
  const activeItem = items.find(it => it.id === activeItemId);
  const activeState = activeItemId ? itemStates[activeItemId] : null;

  return (
    <div className="grid grid-cols-[220px_minmax(0,1fr)] gap-4 items-start">
      {/* 左：资产列表 */}
      <div className="rounded-xl border border-slate-200 bg-white p-3 space-y-1 sticky top-24">
        <div className="flex items-center justify-between px-1 mb-2">
          <p className="text-xs font-medium text-slate-400">资产列表</p>
          <button
            onClick={onAutoSelectToggle}
            className={cn(
              "flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium transition-colors",
              autoSelectFirst ? "bg-violet-700 text-white" : "bg-slate-100 text-slate-400 hover:bg-slate-200"
            )}
            title="自动选用第一张生成图，无需手动确认"
          >
            <Wand2 size={10} />
            自动选图
          </button>
        </div>

        {awaitingCount > 0 && !autoSelectFirst && (
          <div className="text-xs text-violet-700 bg-violet-50 border border-violet-200 rounded-lg px-2.5 py-1.5 mb-2 font-medium">
            {awaitingCount} 个图片等待选择
          </div>
        )}
        {approvalCount > 0 && (
          <div className="text-xs text-violet-600 bg-violet-50 border border-violet-200 rounded-lg px-2.5 py-1.5 mb-2 font-medium">
            {approvalCount} 个资产等待审批
          </div>
        )}

        {items.map(item => {
          const state = itemStates[item.id];
          const status: ItemStatus = state?.status ?? "pending";
          const isActive = item.id === activeItemId;
          const needsAction = status === "awaiting_selection";

          return (
            <button
              key={item.id}
              onClick={() => setActiveItemId(item.id)}
              className={cn(
                "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-left transition-colors",
                isActive ? "bg-violet-50 border border-violet-300" : "hover:bg-slate-50 border border-transparent",
                needsAction && !isActive && "border-violet-200 bg-violet-50/70",
              )}
            >
              <span className="shrink-0">{STATUS_ICONS[status]}</span>
              <div className="flex-1 min-w-0">
                <p className={cn("text-xs font-medium truncate", isActive ? "text-violet-700" : "text-slate-700")}>
                  {item.name}
                </p>
                <p className="text-xs text-slate-400">{TYPE_LABELS[item.type] ?? item.type}</p>
              </div>
              {needsAction && (
                <span className="w-2 h-2 rounded-full bg-violet-600 shrink-0 animate-pulse" />
              )}
            </button>
          );
        })}

        {/* 全局进度日志（折叠） */}
        {(batchLog.length > 0 || currentBatchStage) && stage === "executing" && (
          <div className="mt-2 pt-2 border-t border-slate-100">
            <StageStatus current={currentBatchStage} history={batchStageHistory} />
            <div className="max-h-28 overflow-y-auto space-y-0.5">
              {batchLog.slice(-8).map((line, i) => (
                <p key={i} className="text-xs text-slate-400 font-mono leading-relaxed truncate">{line}</p>
              ))}
            </div>
          </div>
        )}

        {stage === "done" && batchResult && (
          <div className="mt-2 pt-2 border-t border-slate-100 space-y-2">
            {approvalCount > 0 ? (
              <p className="text-xs text-violet-600 font-medium px-1">等待审批通过后继续执行</p>
            ) : batchResult.error === 0 ? (
              <p className="text-xs text-green-600 font-medium px-1">✓ 全部完成</p>
            ) : (
              <p className="text-xs text-red-500 px-1">
                {batchResult.success} 成功 / {batchResult.error} 失败
              </p>
            )}
            {approvalCount === 0 && <BuildDeploy projectRoot={projectRoot} />}
            <button
              onClick={onReset}
              className="w-full py-1.5 rounded-lg border border-slate-200 text-slate-400 hover:text-violet-700 hover:border-violet-300 text-xs transition-colors flex items-center justify-center gap-1"
            >
              <RotateCcw size={11} /> 新建 Mod
            </button>
          </div>
        )}
      </div>

      {/* 右：当前 item 详情 */}
      <div className="rounded-xl border border-slate-200 bg-white p-5 min-h-[300px]">
        {!activeItem || !activeState ? (
          <div className="space-y-3">
            {batchLog.length > 0
              ? <AgentLog lines={batchLog} />
              : <div className="flex items-center justify-center h-48 text-slate-300 text-sm">从左侧选择一个资产查看详情</div>
            }
          </div>
        ) : (
          <ItemDetailPanel
            item={activeItem}
            state={activeState}
            onSelectImage={(idx) => onSelectImage(activeItem.id, idx)}
            onGenerateMore={() => onGenerateMore(activeItem.id)}
            onRetryItem={() => onRetryItem(activeItem.id)}
            approvalBusyActionId={approvalBusyActionId}
            onApproveAction={onApproveAction}
            onRejectAction={onRejectAction}
            onExecuteAction={onExecuteAction}
            onProceedApproval={() => onProceedApproval(activeItem.id)}
            proceedApprovalDisabled={!hasLiveSession || !canProceedBatchApproval(activeState.approvalRequests)}
            onUpdatePrompt={(p) => onUpdatePrompt(activeItem.id, p)}
            onToggleMorePrompt={() => onToggleMorePrompt(activeItem.id)}
          />
        )}
      </div>
    </div>
  );
}

// ── 单个资产详情面板 ──────────────────────────────────────────────────────────

function ItemDetailPanel({
  item, state,
  onSelectImage, onGenerateMore, onRetryItem, approvalBusyActionId, onApproveAction, onRejectAction, onExecuteAction,
  onProceedApproval, proceedApprovalDisabled, onUpdatePrompt, onToggleMorePrompt,
}: {
  item: PlanItem;
  state: ItemState;
  onSelectImage: (idx: number) => void;
  onGenerateMore: () => void;
  onRetryItem: () => void;
  approvalBusyActionId: string | null;
  onApproveAction: (actionId: string) => void;
  onRejectAction: (actionId: string) => void;
  onExecuteAction: (actionId: string) => void;
  onProceedApproval: () => void;
  proceedApprovalDisabled: boolean;
  onUpdatePrompt: (p: string) => void;
  onToggleMorePrompt: () => void;
}) {
  const [showTrace, setShowTrace] = useState(false);

  return (
    <div className="space-y-4">
      {/* 标题行 */}
      <div className="flex items-center gap-3">
        <span className="text-xs font-medium text-slate-400 bg-slate-100 rounded px-1.5 py-0.5">
          {TYPE_LABELS[item.type] ?? item.type}
        </span>
        <h3 className="font-bold text-slate-800">{item.name}</h3>
        <span className={cn(
          "ml-auto text-xs px-2 py-0.5 rounded-full font-medium",
          state.status === "done"               ? "bg-green-100 text-green-700" :
          state.status === "error"              ? "bg-red-100 text-red-600" :
          state.status === "awaiting_selection" ? "bg-violet-100 text-violet-700" :
          state.status === "approval_pending"   ? "bg-violet-100 text-violet-700" :
          state.status === "code_generating"    ? "bg-blue-100 text-blue-600" :
                                                  "bg-slate-100 text-slate-500"
        )}>
          {STATUS_LABELS[state.status]}
        </span>
      </div>

      {/* 进度日志 */}
      <StageStatus current={state.currentStage} history={state.stageHistory} isComplete={state.status === "done"} />
      {state.progress.length > 0 && (
        <AgentLog lines={state.progress} />
      )}

      {state.status === "approval_pending" && (
        <ApprovalPanel
          summary={state.approvalSummary}
          requests={state.approvalRequests}
          busyActionId={approvalBusyActionId}
          onApprove={onApproveAction}
          onReject={onRejectAction}
          onExecute={onExecuteAction}
          onProceed={onProceedApproval}
          proceedDisabled={proceedApprovalDisabled}
        />
      )}

      {/* 图片画廊（等待选择时） */}
      {state.images.length > 0 && state.status !== "done" && (
        <div className="space-y-3">
          <p className="text-xs font-medium text-slate-500">
            {state.status === "awaiting_selection" ? "请选择一张图片" : "已选图片"}
          </p>
          <div className="flex flex-wrap gap-3">
            {state.images.map((b64, i) => (
              <div
                key={i}
                className="group relative rounded-lg overflow-hidden border border-slate-200 hover:border-violet-400 transition-colors"
                style={{ width: state.images.length === 1 ? "240px" : "180px" }}
              >
                <img src={`data:image/png;base64,${b64}`} alt={`图 ${i + 1}`} className="w-full h-auto block" />
                {state.status === "awaiting_selection" && (
                  <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
                    <button
                      onClick={() => onSelectImage(i)}
                      className="py-1.5 px-4 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-600 transition-colors shadow-lg"
                    >
                      用这张
                    </button>
                  </div>
                )}
                <div className="absolute top-1 left-1 w-5 h-5 rounded-full bg-black/50 text-white text-xs flex items-center justify-center font-bold">
                  {i + 1}
                </div>
              </div>
            ))}
          </div>

          {/* 再来一张 */}
          {state.status === "awaiting_selection" && (
            <div className="space-y-2 pt-2 border-t border-slate-100">
              <button
                onClick={() => onSelectImage(0)}
                className="w-full py-1.5 rounded-lg bg-violet-700 text-white text-sm font-bold hover:bg-violet-800 transition-colors"
              >
                选第一张
              </button>
              <button
                onClick={onToggleMorePrompt}
                className="text-xs text-slate-400 hover:text-violet-700 flex items-center gap-1 transition-colors"
              >
                {state.showMorePrompt ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                {state.showMorePrompt ? "收起" : "修改提示词"}
              </button>
              {state.showMorePrompt && (
                <textarea
                  value={state.currentPrompt}
                  onChange={e => onUpdatePrompt(e.target.value)}
                  rows={3}
                  className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-xs font-mono resize-none focus:outline-none focus:border-violet-400"
                />
              )}
              <button
                onClick={onGenerateMore}
                className="w-full py-1.5 rounded-lg border border-violet-300 text-violet-700 text-sm hover:bg-violet-50 transition-colors"
              >
                再来一张
              </button>
            </div>
          )}
        </div>
      )}

      {/* Code Agent 日志 */}
            {state.agentLog.length > 0 && (
              <div className="space-y-1">
                <p className="text-xs font-medium text-slate-500">Code Agent</p>
                <AgentLog
                  lines={state.agentLog}
                  entries={state.agentLogEntries}
                  currentModel={state.currentAgentModel}
                />
              </div>
            )}

      {/* 完成提示 */}
      {state.status === "done" && (
        <p className="text-sm text-green-600 font-medium">✓ {item.name} 创建完成</p>
      )}

      {/* 错误 */}
      {state.status === "error" && state.error && (
        <div className="rounded-lg border border-red-200 bg-red-50 p-3 space-y-2">
          <div className="flex items-start gap-2">
            <AlertTriangle size={14} className="text-red-500 shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              <p className="text-xs font-semibold text-red-700">执行失败</p>
              <pre className="text-xs text-red-600 font-mono mt-1 whitespace-pre-wrap break-all">{state.error}</pre>
            </div>
          </div>
          <button
            onClick={onRetryItem}
            className="w-full py-1.5 rounded-lg bg-red-500 text-white text-sm font-bold hover:bg-red-600 transition-colors flex items-center justify-center gap-1"
          >
            <RotateCcw size={13} /> 重新生成此资产
          </button>
          {state.errorTrace && (
            <>
              <button
                onClick={() => setShowTrace(v => !v)}
                className="text-xs text-red-400 hover:text-red-600 flex items-center gap-1"
              >
                {showTrace ? <ChevronUp size={11} /> : <ChevronDown size={11} />}
                {showTrace ? "收起" : "展开"} Traceback
              </button>
              {showTrace && (
                <pre className="text-xs text-red-500/80 font-mono whitespace-pre-wrap break-all max-h-48 overflow-y-auto bg-red-100/30 rounded p-2">
                  {state.errorTrace}
                </pre>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
