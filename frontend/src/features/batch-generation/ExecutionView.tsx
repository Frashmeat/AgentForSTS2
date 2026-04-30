// 批量生成执行视图：左侧资产列表（带状态徽章 + 自动选图开关 + 全局进度日志）+ 右侧选中 item 详情。
// 从 view.tsx 抽出，纯 props-driven。

import { RotateCcw, StopCircle, Wand2 } from "lucide-react";

import { AgentLog } from "../../components/AgentLog";
import { BuildDeploy } from "../../components/BuildDeploy";
import { StageStatus } from "../../components/StageStatus";
import type { PlanItem } from "../../lib/batch_ws";
import { cn } from "../../lib/utils";
import { canProceedBatchApproval } from "./approval";
import { ItemDetailPanel } from "./ItemDetailPanel.tsx";
import type { BatchItemState as ItemState, BatchItemStatus as ItemStatus } from "./state.ts";
import type { WorkspaceFeatureProps } from "../workspace/types.ts";
import { TYPE_LABELS } from "./view-constants.ts";
import { STATUS_ICONS } from "./view-icons.tsx";

export function ExecutionView({
  items,
  itemStates,
  activeItemId,
  setActiveItemId,
  batchLog,
  currentBatchStage,
  batchStageHistory,
  batchResult,
  stage,
  projectRoot,
  onStatusNotice,
  autoSelectFirst,
  onAutoSelectToggle,
  onSelectImage,
  onGenerateMore,
  onRetryItem,
  approvalBusyActionId,
  onApproveAction,
  onRejectAction,
  onExecuteAction,
  onProceedApproval,
  hasLiveSession,
  onCancelWorkflow,
  onUpdatePrompt,
  onToggleMorePrompt,
  onReset,
}: {
  items: PlanItem[];
  itemStates: Record<string, ItemState>;
  activeItemId: string | null;
  setActiveItemId: (id: string) => void;
  batchLog: string[];
  currentBatchStage: string | null;
  batchStageHistory: string[];
  batchResult: { success: number; error: number } | null;
  stage: "executing" | "cancelled" | "done";
  projectRoot: string;
  onStatusNotice: WorkspaceFeatureProps["onStatusNotice"];
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
  onCancelWorkflow: () => void;
  onUpdatePrompt: (id: string, prompt: string) => void;
  onToggleMorePrompt: (id: string) => void;
  onReset: () => void;
}) {
  const awaitingCount = items.filter((it) => itemStates[it.id]?.status === "awaiting_selection").length;
  const approvalCount = items.filter((it) => itemStates[it.id]?.status === "approval_pending").length;
  const activeItem = items.find((it) => it.id === activeItemId);
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
              autoSelectFirst ? "bg-violet-700 text-white" : "bg-slate-100 text-slate-400 hover:bg-slate-200",
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
        {hasLiveSession && stage === "executing" && (
          <button
            onClick={onCancelWorkflow}
            className="w-full py-1.5 rounded-lg border border-red-200 text-red-600 hover:bg-red-50 text-xs transition-colors flex items-center justify-center gap-1 mb-2"
          >
            <StopCircle size={11} />
            停止生成
          </button>
        )}

        {items.map((item) => {
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
              {needsAction && <span className="w-2 h-2 rounded-full bg-violet-600 shrink-0 animate-pulse" />}
            </button>
          );
        })}

        {/* 全局进度日志（折叠） */}
        {(batchLog.length > 0 || currentBatchStage) && stage === "executing" && (
          <div className="mt-2 pt-2 border-t border-slate-100">
            <StageStatus current={currentBatchStage} history={batchStageHistory} />
            <div className="max-h-28 overflow-y-auto space-y-0.5">
              {batchLog.slice(-8).map((line, i) => (
                <p key={i} className="text-xs text-slate-400 font-mono leading-relaxed truncate">
                  {line}
                </p>
              ))}
            </div>
          </div>
        )}

        {stage === "cancelled" && (
          <div className="mt-2 pt-2 border-t border-slate-100 space-y-2">
            <p className="text-xs text-slate-600 font-medium px-1">已取消当前生成</p>
            <button
              onClick={onReset}
              className="w-full py-1.5 rounded-lg border border-slate-200 text-slate-400 hover:text-violet-700 hover:border-violet-300 text-xs transition-colors flex items-center justify-center gap-1"
            >
              <RotateCcw size={11} /> 新建 Mod
            </button>
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
            {approvalCount === 0 && <BuildDeploy projectRoot={projectRoot} onStatusNotice={onStatusNotice} />}
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
            {batchLog.length > 0 ? (
              <AgentLog lines={batchLog} />
            ) : (
              <div className="flex items-center justify-center h-48 text-slate-300 text-sm">
                从左侧选择一个资产查看详情
              </div>
            )}
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
