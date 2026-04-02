import { useState, useRef, useEffect, useReducer } from "react";
import {
  Loader2, ChevronDown, ChevronUp, RotateCcw,
  CheckCircle2, XCircle, Clock, ImageIcon, Code2, Sparkles, AlertTriangle,
  Upload, Wand2,
} from "lucide-react";
import { ApprovalPanel } from "../components/ApprovalPanel";
import { ProjectRootField } from "../components/ProjectRootField";
import { approveApproval, executeApproval, rejectApproval, type ApprovalRequest } from "../shared/api/index.ts";
import { BatchSocket, PlanItem, ModPlan } from "../lib/batch_ws";
import { AgentLog } from "../components/AgentLog";
import { StageStatus } from "../components/StageStatus";
import { BuildDeploy } from "../components/BuildDeploy";
import { cn } from "../lib/utils";
import { runApprovalAction } from "../shared/approvalAction.ts";
import { useDefaultProjectRoot } from "../shared/useDefaultProjectRoot.ts";
import { useProjectCreation } from "../shared/useProjectCreation.ts";
import { createBatchPlanningController } from "../features/batch-generation/planningController.ts";
import {
  batchWorkflowReducer,
  createInitialBatchRuntimeState,
  type BatchItemState,
  type BatchRuntimeState,
} from "../features/batch-generation/state";
import {
  clearBatchRuntimeSnapshot,
  createRetryableBatchItemState,
  loadBatchRuntimeSnapshot,
  refreshRecoveredBatchApprovals,
  saveBatchRuntimeSnapshot,
} from "../features/batch-generation/recovery";

type ItemStatus = BatchItemState["status"];
type ItemState = BatchItemState;

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
  img_generating:     <Loader2 size={14} className="text-amber-400 animate-spin" />,
  awaiting_selection: <ImageIcon size={14} className="text-amber-500" />,
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

// ── 主组件 ────────────────────────────────────────────────────────────────────

function BatchModePage() {
  const [requirements, setRequirements] = useState("");
  const [projectRoot, setProjectRoot] = useState("");

  useDefaultProjectRoot({
    setProjectRoot,
  });

  const [plan, setPlan] = useState<ModPlan | null>(() => {
    try { const s = localStorage.getItem("ats_last_plan"); return s ? JSON.parse(s) : null; } catch { return null; }
  });
  const [editedItems, setEditedItems] = useState<PlanItem[]>(() => {
    try { const s = localStorage.getItem("ats_last_plan_items"); return s ? JSON.parse(s) : []; } catch { return []; }
  });
  const [initialRuntimeSnapshot] = useState<BatchRuntimeState | null>(() => (
    plan || editedItems.length > 0 ? loadBatchRuntimeSnapshot() : null
  ));
  const [runtimeState, dispatchRuntime] = useReducer(
    batchWorkflowReducer,
    undefined,
    () => initialRuntimeSnapshot ?? createInitialBatchRuntimeState(),
  );
  const [restoredSnapshotMode, setRestoredSnapshotMode] = useState(() => initialRuntimeSnapshot !== null);
  const [restoredApprovalRefreshPending, setRestoredApprovalRefreshPending] = useState(() => initialRuntimeSnapshot !== null);
  const {
    projectCreateBusy,
    projectCreateMessage,
    projectCreateError,
    clearProjectCreationFeedback,
    resetProjectCreationState,
    createProjectAtRoot,
  } = useProjectCreation({
    onProjectCreated: setProjectRoot,
  });

  const [autoSelectFirst, setAutoSelectFirst] = useState(false);
  const autoSelectRef = useRef(false);
  useEffect(() => { autoSelectRef.current = autoSelectFirst; }, [autoSelectFirst]);
  const socketRef = useRef<BatchSocket | null>(null);
  const planningControllerRef = useRef<ReturnType<typeof createBatchPlanningController> | null>(null);

  useEffect(() => {
    if (!plan || editedItems.length === 0) {
      return;
    }
    const itemsForStorage = editedItems.map((item) => ({ ...item, provided_image_b64: undefined }));
    try {
      localStorage.setItem("ats_last_plan_items", JSON.stringify(itemsForStorage));
    } catch {}
  }, [plan, editedItems]);

  useEffect(() => {
    const shouldPersistRuntime = runtimeState.stage === "review_plan" || runtimeState.stage === "executing" || runtimeState.stage === "done";
    const hasContext = Boolean(plan) || editedItems.length > 0;
    if (!shouldPersistRuntime || !hasContext) {
      clearBatchRuntimeSnapshot();
      return;
    }
    saveBatchRuntimeSnapshot(localStorage, runtimeState);
  }, [editedItems.length, plan, runtimeState]);

  useEffect(() => {
    if (!restoredApprovalRefreshPending) {
      return;
    }
    let cancelled = false;

    void refreshRecoveredBatchApprovals(initialRuntimeSnapshot ?? runtimeState).then((refreshedState) => {
      if (cancelled) {
        return;
      }
      for (const [itemId, itemState] of Object.entries(refreshedState.itemStates)) {
        const currentState = runtimeState.itemStates[itemId];
        if (!currentState) {
          continue;
        }
        if (
          currentState.status === itemState.status &&
          JSON.stringify(currentState.approvalRequests) === JSON.stringify(itemState.approvalRequests)
        ) {
          continue;
        }
        dispatchRuntime({
          type: "item_state_patched",
          itemId,
          patch: {
            status: itemState.status,
            approvalRequests: itemState.approvalRequests,
          },
        });
      }
      setRestoredApprovalRefreshPending(false);
    });

    return () => {
      cancelled = true;
    };
  }, [initialRuntimeSnapshot, restoredApprovalRefreshPending, runtimeState]);

  // ── Start ─────────────────────────────────────────────────────────────────

  function applyGeneratedPlan(nextPlan: ModPlan) {
    setPlan(nextPlan);
    setEditedItems(nextPlan.items);
    dispatchRuntime({ type: "plan_ready_received" });
    try {
      localStorage.setItem("ats_last_plan", JSON.stringify(nextPlan));
      localStorage.setItem("ats_last_plan_items", JSON.stringify(nextPlan.items));
    } catch {}
  }

  function _registerBatchHandlers(ws: BatchSocket) {
    ws.on("planning", () => dispatchRuntime({ type: "batch_log_appended", message: "正在规划 Mod..." }));
    ws.on("plan_ready", (d) => {
      applyGeneratedPlan(d.plan);
    });
    ws.on("batch_progress", (d) => dispatchRuntime({ type: "batch_log_appended", message: d.message }));
    ws.on("stage_update", (d) => {
      if (d.item_id) {
        dispatchRuntime({ type: "item_stage_message", itemId: d.item_id!, message: d.message });
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
      dispatchRuntime({ type: "item_progress_received", itemId: d.item_id, message: d.message });
    });
    ws.on("item_image_ready", (d) => {
      dispatchRuntime({ type: "item_image_ready", itemId: d.item_id, image: d.image, index: d.index, prompt: d.prompt });
      if (autoSelectRef.current) {
        ws.send({ action: "select_image", item_id: d.item_id, index: 0 });
        dispatchRuntime({ type: "item_state_patched", itemId: d.item_id, patch: { status: "code_generating" } });
      }
    });
    ws.on("item_agent_stream", (d) => { dispatchRuntime({ type: "item_agent_stream", itemId: d.item_id, chunk: d.chunk }); });
    ws.on("item_approval_pending", (d) => {
      dispatchRuntime({ type: "item_approval_pending", itemId: d.item_id, summary: d.summary, requests: d.requests });
    });
    ws.on("item_done", (d) => {
      dispatchRuntime({ type: "item_done", itemId: d.item_id });
    });
    ws.on("item_error", (d) => {
      dispatchRuntime({ type: "item_error", itemId: d.item_id, message: d.message, traceback: d.traceback ?? null });
    });
    ws.on("batch_done", (d) => {
      dispatchRuntime({ type: "batch_done", success: d.success_count, error: d.error_count });
    });
    ws.on("error", (d) => { dispatchRuntime({ type: "workflow_failed", message: d.message }); });
  }

  if (!planningControllerRef.current) {
    planningControllerRef.current = createBatchPlanningController({
      closeSocket() {
        socketRef.current?.close();
      },
      setSocket(socket) {
        socketRef.current = socket as BatchSocket | null;
      },
      clearProjectCreationFeedback,
      setRestoredSnapshotMode,
      setRestoredApprovalRefreshPending,
      dispatchPlanningStarted() {
        dispatchRuntime({ type: "planning_started" });
      },
      clearPlan() {
        setPlan(null);
      },
      applyGeneratedPlan,
      registerSocketHandlers(socket) {
        _registerBatchHandlers(socket as BatchSocket);
      },
      reportWorkflowError(message) {
        dispatchRuntime({ type: "workflow_failed", message });
      },
    });
  }
  const planningController = planningControllerRef.current;

  async function confirmPlan() {
    if (!plan) return;
    setRestoredSnapshotMode(false);
    setRestoredApprovalRefreshPending(false);
    const itemsForStorage = editedItems.map(it => ({ ...it, provided_image_b64: undefined }));
    try { localStorage.setItem("ats_last_plan_items", JSON.stringify(itemsForStorage)); } catch {}

    if (!socketRef.current) {
      // 恢复的规划：重新建连接，直接跳到执行
      const ws = new BatchSocket();
      socketRef.current = ws;
      _registerBatchHandlers(ws);
      try {
        await ws.waitOpen();
      } catch (error) {
        dispatchRuntime({ type: "workflow_failed", message: error instanceof Error ? error.message : String(error) });
        return;
      }
      dispatchRuntime({ type: "stage_set", stage: "executing" });
      ws.send({ action: "start_with_plan", project_root: projectRoot, plan: { ...plan, items: editedItems } });
    } else {
      dispatchRuntime({ type: "stage_set", stage: "executing" });
      socketRef.current.send({ action: "confirm_plan", plan: { ...plan, items: editedItems } });
    }
  }

  function handleSelectImage(itemId: string, index: number) {
    if (!socketRef.current) return;
    socketRef.current.send({ action: "select_image", item_id: itemId, index });
    dispatchRuntime({ type: "item_state_patched", itemId, patch: { status: "code_generating" } });
    const nextAwaiting = editedItems.find(
      it => it.id !== itemId && runtimeState.itemStates[it.id]?.status === "awaiting_selection"
    );
    if (nextAwaiting) dispatchRuntime({ type: "active_item_set", itemId: nextAwaiting.id });
  }

  function handleRetryItem(itemId: string) {
    if (!socketRef.current) return;
    socketRef.current.send({ action: "retry_item", item_id: itemId });
    dispatchRuntime({
      type: "item_state_patched",
      itemId,
      patch: createRetryableBatchItemState(runtimeState.itemStates[itemId]),
    });
  }

  function handleGenerateMore(itemId: string) {
    if (!socketRef.current) return;
    const state = runtimeState.itemStates[itemId];
    socketRef.current.send({
      action: "generate_more",
      item_id: itemId,
      prompt: state?.currentPrompt,
    });
    dispatchRuntime({ type: "item_state_patched", itemId, patch: { status: "img_generating", showMorePrompt: false } });
  }

  function reset() {
    socketRef.current?.close();
    socketRef.current = null;
    clearBatchRuntimeSnapshot();
    resetProjectCreationState();
    setRestoredSnapshotMode(false);
    setRestoredApprovalRefreshPending(false);
    setPlan(null);
    setEditedItems([]);
    dispatchRuntime({ type: "workflow_reset" });
  }

  async function handleApprovalAction(
    actionId: string,
    action: (id: string) => Promise<ApprovalRequest>,
  ) {
    await runApprovalAction({
      actionId,
      action,
      onBusyChange(nextActionId) {
        dispatchRuntime({ type: "approval_busy_set", actionId: nextActionId });
      },
      onSuccess(updated) {
        dispatchRuntime({ type: "approval_request_updated", actionId, request: updated });
      },
      onError(message) {
        dispatchRuntime({ type: "workflow_failed", message });
      },
    });
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="space-y-5">
      {/* 输入阶段 */}
      {runtimeState.stage === "input" && (
        <div className="rounded-xl border border-amber-300 bg-white shadow-md p-5 space-y-4">
          <h2 className="font-semibold text-slate-800">描述你的 Mod 需求</h2>
          <p className="text-xs text-slate-400">
            用自然语言描述整个 Mod 的内容，AI 会自动规划需要哪些卡牌、遗物、机制，并逐一创建。
          </p>
          <textarea
            value={requirements}
            onChange={e => setRequirements(e.target.value)}
            rows={6}
            placeholder={"例如：\n我想做一个暗法师角色，主题是腐化和献祭。\n包含3张卡牌（攻击、技能、力量各一张），\n2个遗物（战斗开始触发），\n以及一个腐化叠层的 buff 机制。"}
            className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-amber-400 focus:ring-1 focus:ring-amber-100 resize-none"
          />
          <ProjectRootField
            value={projectRoot}
            placeholder="E:/STS2mod"
            createActionLabel="创建项目"
            createBusy={projectCreateBusy}
            createMessage={projectCreateMessage}
            createError={projectCreateError}
            onChange={setProjectRoot}
            onCreateProject={() => { void createProjectAtRoot(projectRoot).catch(() => {}); }}
          />
          <div className="grid gap-2 sm:grid-cols-2">
            <button
              onClick={() => { void planningController.startSocketPlanning(requirements, projectRoot); }}
              disabled={!requirements.trim() || !projectRoot.trim()}
              className="w-full py-2.5 rounded-lg bg-amber-500 text-white font-bold text-sm hover:bg-amber-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center justify-center gap-2"
            >
              <Sparkles size={15} />
              规划 Mod
            </button>
            <button
              onClick={() => { void planningController.startHttpPlanning(requirements, projectRoot); }}
              disabled={!requirements.trim() || !projectRoot.trim()}
              className="w-full py-2.5 rounded-lg border border-amber-300 text-amber-700 font-bold text-sm hover:bg-amber-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              快速规划（HTTP）
            </button>
          </div>
          {plan && (
            <button
              onClick={() => dispatchRuntime({ type: "stage_set", stage: "review_plan" })}
              className="w-full py-2 rounded-lg border border-amber-200 text-amber-600 text-sm hover:bg-amber-50 transition-colors"
            >
              恢复上次规划：{plan.mod_name}
            </button>
          )}
        </div>
      )}

      {/* 规划中 */}
      {runtimeState.stage === "planning" && (
        <div className="rounded-xl border border-slate-200 bg-white p-5 space-y-3">
          <div className="flex items-center gap-2.5">
            <Loader2 size={16} className="text-amber-500 animate-spin" />
            <span className="text-sm font-medium text-slate-600">AI 正在规划 Mod...</span>
          </div>
          {runtimeState.batchLog.length > 0 && <AgentLog lines={runtimeState.batchLog} />}
        </div>
      )}

      {/* 审阅计划 */}
      {runtimeState.stage === "review_plan" && plan && (
        <ReviewPlan
          plan={plan}
          editedItems={editedItems}
          setEditedItems={setEditedItems}
          onConfirm={confirmPlan}
          onReset={reset}
        />
      )}

      {/* 全局错误 */}
      {runtimeState.stage === "error" && (
        <div className="rounded-xl border border-red-200 bg-red-50 p-5 space-y-3">
          <div className="flex items-start gap-2">
            <AlertTriangle size={16} className="text-red-500 shrink-0 mt-0.5" />
            <div>
              <p className="text-sm font-semibold text-red-700">规划失败</p>
              {runtimeState.globalError && <pre className="text-xs text-red-600 font-mono mt-1 whitespace-pre-wrap">{runtimeState.globalError}</pre>}
            </div>
          </div>
          <button onClick={reset} className="text-sm text-red-500 hover:text-red-700 flex items-center gap-1">
            <RotateCcw size={13} /> 重试
          </button>
        </div>
      )}

      {/* 执行阶段 + 完成 */}
      {(runtimeState.stage === "executing" || runtimeState.stage === "done") && (
        <ExecutionView
          items={editedItems}
          itemStates={runtimeState.itemStates}
          activeItemId={runtimeState.activeItemId}
          setActiveItemId={(id) => dispatchRuntime({ type: "active_item_set", itemId: id })}
          batchLog={runtimeState.batchLog}
          currentBatchStage={runtimeState.currentBatchStage}
          batchStageHistory={runtimeState.batchStageHistory}
          batchResult={runtimeState.batchResult}
          stage={runtimeState.stage}
          projectRoot={projectRoot}
          hasLiveSession={Boolean(socketRef.current)}
          showRecoveredNotice={restoredSnapshotMode && !socketRef.current}
          canRestartExecution={Boolean(plan) && editedItems.length > 0}
          onRestartExecution={() => { void confirmPlan(); }}
          autoSelectFirst={autoSelectFirst}
          onAutoSelectToggle={() => setAutoSelectFirst(v => !v)}
          onSelectImage={handleSelectImage}
          onGenerateMore={handleGenerateMore}
          onRetryItem={handleRetryItem}
          approvalBusyActionId={runtimeState.approvalBusyActionId}
          onApproveAction={(actionId) => { void handleApprovalAction(actionId, approveApproval); }}
          onRejectAction={(actionId) => { void handleApprovalAction(actionId, (id) => rejectApproval(id)); }}
          onExecuteAction={(actionId) => { void handleApprovalAction(actionId, executeApproval); }}
          onUpdatePrompt={(id, prompt) =>
            dispatchRuntime({ type: "item_state_patched", itemId: id, patch: { currentPrompt: prompt } })
          }
          onToggleMorePrompt={(id) =>
            dispatchRuntime({
              type: "item_state_patched",
              itemId: id,
              patch: { showMorePrompt: !runtimeState.itemStates[id]?.showMorePrompt },
            })
          }
          onReset={reset}
        />
      )}
    </div>
  );
}

export function BatchGenerationFeatureView() {
  return <BatchModePage />;
}

export default function BatchMode() {
  return <BatchGenerationFeatureView />;
}

// ── 计划审阅组件 ──────────────────────────────────────────────────────────────

function ReviewPlan({
  plan, editedItems, setEditedItems, onConfirm, onReset,
}: {
  plan: ModPlan;
  editedItems: PlanItem[];
  setEditedItems: (items: PlanItem[]) => void;
  onConfirm: () => void;
  onReset: () => void;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [uploadPreviews, setUploadPreviews] = useState<Record<string, string>>({});

  function updateItem(id: string, patch: Partial<PlanItem>) {
    setEditedItems(editedItems.map(it => it.id === id ? { ...it, ...patch } : it));
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
      <div className="rounded-xl border border-amber-300 bg-white shadow-md p-5">
        <div className="flex items-start justify-between mb-4">
          <div>
            <h2 className="font-bold text-slate-800">{plan.mod_name}</h2>
            <p className="text-xs text-slate-500 mt-0.5">{plan.summary}</p>
          </div>
          <span className="text-xs text-amber-600 bg-amber-50 border border-amber-200 rounded-full px-2 py-0.5 font-medium">
            {editedItems.length} 个资产
          </span>
        </div>

        <div className="space-y-2">
          {editedItems.map(item => (
            <div key={item.id} className="rounded-lg border border-slate-200 bg-slate-50 overflow-hidden">
              <button
                className="w-full flex items-center gap-3 px-3 py-2.5 text-left hover:bg-slate-100 transition-colors"
                onClick={() => setExpandedId(expandedId === item.id ? null : item.id)}
              >
                <span className="text-xs font-medium text-slate-400 bg-slate-200 rounded px-1.5 py-0.5 shrink-0">
                  {TYPE_LABELS[item.type] ?? item.type}
                </span>
                <span className="text-sm font-medium text-slate-700 flex-1">{item.name}</span>
                {item.depends_on.length > 0 && (
                  <span className="text-xs text-slate-400">依赖 {item.depends_on.length}</span>
                )}
                {expandedId === item.id
                  ? <ChevronUp size={13} className="text-slate-400 shrink-0" />
                  : <ChevronDown size={13} className="text-slate-400 shrink-0" />
                }
              </button>

              {expandedId === item.id && (
                <div className="px-3 pb-3 space-y-2 border-t border-slate-200 pt-2.5">
                  <div className="space-y-1">
                    <label className="text-xs text-slate-400">名称（英文）</label>
                    <input
                      value={item.name}
                      onChange={e => updateItem(item.id, { name: e.target.value })}
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm focus:outline-none focus:border-amber-400"
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs text-slate-400">描述</label>
                    <textarea
                      value={item.description}
                      onChange={e => updateItem(item.id, { description: e.target.value })}
                      rows={2}
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-amber-400"
                    />
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
                              ? "bg-amber-500 text-white"
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
                              ? "bg-amber-500 text-white"
                              : "bg-slate-100 text-slate-500 hover:bg-slate-200"
                          )}
                        >
                          <Upload size={11} /> 上传图片
                        </button>
                      </div>
                      {/* 上传预览 */}
                      {item.provided_image_b64 && uploadPreviews[item.id] && (
                        <div className="relative w-24 h-24 rounded-lg overflow-hidden border border-amber-300">
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
                            className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-sm resize-none focus:outline-none focus:border-amber-400"
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
                      className="w-full bg-white border border-slate-200 rounded px-2 py-1 text-xs font-mono resize-none focus:outline-none focus:border-amber-400"
                    />
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>

        <div className="flex gap-2 mt-4">
          <button
            onClick={onConfirm}
            className="flex-1 py-2.5 rounded-lg bg-amber-500 text-white font-bold text-sm hover:bg-amber-600 transition-colors"
          >
            确认，开始执行
          </button>
          <button
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
  hasLiveSession, showRecoveredNotice, canRestartExecution, onRestartExecution,
  autoSelectFirst, onAutoSelectToggle,
  onSelectImage, onGenerateMore, onRetryItem, approvalBusyActionId, onApproveAction, onRejectAction, onExecuteAction, onUpdatePrompt, onToggleMorePrompt, onReset,
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
  hasLiveSession: boolean;
  showRecoveredNotice: boolean;
  canRestartExecution: boolean;
  onRestartExecution: () => void;
  autoSelectFirst: boolean;
  onAutoSelectToggle: () => void;
  onSelectImage: (id: string, idx: number) => void;
  onGenerateMore: (id: string) => void;
  onRetryItem: (id: string) => void;
  approvalBusyActionId: string | null;
  onApproveAction: (actionId: string) => void;
  onRejectAction: (actionId: string) => void;
  onExecuteAction: (actionId: string) => void;
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
              autoSelectFirst ? "bg-amber-500 text-white" : "bg-slate-100 text-slate-400 hover:bg-slate-200"
            )}
            title="自动选用第一张生成图，无需手动确认"
          >
            <Wand2 size={10} />
            自动选图
          </button>
        </div>

        {awaitingCount > 0 && !autoSelectFirst && (
          <div className="text-xs text-amber-600 bg-amber-50 border border-amber-200 rounded-lg px-2.5 py-1.5 mb-2 font-medium">
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
                isActive ? "bg-amber-50 border border-amber-300" : "hover:bg-slate-50 border border-transparent",
                needsAction && !isActive && "border-amber-200 bg-amber-50/50",
              )}
            >
              <span className="shrink-0">{STATUS_ICONS[status]}</span>
              <div className="flex-1 min-w-0">
                <p className={cn("text-xs font-medium truncate", isActive ? "text-amber-700" : "text-slate-700")}>
                  {item.name}
                </p>
                <p className="text-xs text-slate-400">{TYPE_LABELS[item.type] ?? item.type}</p>
              </div>
              {needsAction && (
                <span className="w-2 h-2 rounded-full bg-amber-500 shrink-0 animate-pulse" />
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
              className="w-full py-1.5 rounded-lg border border-slate-200 text-slate-400 hover:text-amber-600 hover:border-amber-300 text-xs transition-colors flex items-center justify-center gap-1"
            >
              <RotateCcw size={11} /> 新建 Mod
            </button>
          </div>
        )}
      </div>

      {/* 右：当前 item 详情 */}
      <div className="rounded-xl border border-slate-200 bg-white p-5 min-h-[300px]">
        {showRecoveredNotice && (
          <div className="mb-4 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2.5 text-xs text-amber-700 space-y-2">
            <p>当前展示的是本地恢复的执行快照。审批状态会自动同步后端，但选图、补图和重试不代表旧 WebSocket 会继续执行。</p>
            {canRestartExecution && (
              <button
                onClick={onRestartExecution}
                className="inline-flex items-center gap-1 rounded-md border border-amber-300 px-2.5 py-1 text-xs font-medium text-amber-700 hover:bg-amber-100 transition-colors"
              >
                <RotateCcw size={11} />
                按当前计划重新开始执行
              </button>
            )}
          </div>
        )}
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
            hasLiveSession={hasLiveSession}
            onSelectImage={(idx) => onSelectImage(activeItem.id, idx)}
            onGenerateMore={() => onGenerateMore(activeItem.id)}
            onRetryItem={() => onRetryItem(activeItem.id)}
            approvalBusyActionId={approvalBusyActionId}
            onApproveAction={onApproveAction}
            onRejectAction={onRejectAction}
            onExecuteAction={onExecuteAction}
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
  hasLiveSession,
  onSelectImage, onGenerateMore, onRetryItem, approvalBusyActionId, onApproveAction, onRejectAction, onExecuteAction, onUpdatePrompt, onToggleMorePrompt,
}: {
  item: PlanItem;
  state: ItemState;
  hasLiveSession: boolean;
  onSelectImage: (idx: number) => void;
  onGenerateMore: () => void;
  onRetryItem: () => void;
  approvalBusyActionId: string | null;
  onApproveAction: (actionId: string) => void;
  onRejectAction: (actionId: string) => void;
  onExecuteAction: (actionId: string) => void;
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
          state.status === "awaiting_selection" ? "bg-amber-100 text-amber-700" :
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
        />
      )}

      {!hasLiveSession && state.status !== "done" && state.status !== "approval_pending" && (
        <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2 text-xs text-slate-500">
          当前是恢复后的本地快照。需要重新开始执行后，才能继续选图、补图或重试该资产。
        </div>
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
                className="group relative rounded-lg overflow-hidden border border-slate-200 hover:border-amber-400 transition-colors"
                style={{ width: state.images.length === 1 ? "240px" : "180px" }}
              >
                <img src={`data:image/png;base64,${b64}`} alt={`图 ${i + 1}`} className="w-full h-auto block" />
                {state.status === "awaiting_selection" && (
                  <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
                    <button
                      disabled={!hasLiveSession}
                      onClick={() => onSelectImage(i)}
                      className="py-1.5 px-4 rounded-lg bg-amber-500 text-white font-bold text-sm hover:bg-amber-400 transition-colors shadow-lg disabled:cursor-not-allowed disabled:bg-slate-400"
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
                disabled={!hasLiveSession}
                onClick={() => onSelectImage(0)}
                className="w-full py-1.5 rounded-lg bg-amber-500 text-white text-sm font-bold hover:bg-amber-600 transition-colors disabled:cursor-not-allowed disabled:bg-slate-400"
              >
                选第一张
              </button>
              <button
                onClick={onToggleMorePrompt}
                className="text-xs text-slate-400 hover:text-amber-600 flex items-center gap-1 transition-colors"
              >
                {state.showMorePrompt ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                {state.showMorePrompt ? "收起" : "修改提示词"}
              </button>
              {state.showMorePrompt && (
                <textarea
                  value={state.currentPrompt}
                  onChange={e => onUpdatePrompt(e.target.value)}
                  rows={3}
                  className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-xs font-mono resize-none focus:outline-none focus:border-amber-400"
                />
              )}
              <button
                disabled={!hasLiveSession}
                onClick={onGenerateMore}
                className="w-full py-1.5 rounded-lg border border-amber-300 text-amber-600 text-sm hover:bg-amber-50 transition-colors disabled:cursor-not-allowed disabled:border-slate-200 disabled:text-slate-400 disabled:bg-slate-50"
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
          <AgentLog lines={state.agentLog} />
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
            disabled={!hasLiveSession}
            onClick={onRetryItem}
            className="w-full py-1.5 rounded-lg bg-red-500 text-white text-sm font-bold hover:bg-red-600 transition-colors flex items-center justify-center gap-1 disabled:cursor-not-allowed disabled:bg-slate-400"
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
