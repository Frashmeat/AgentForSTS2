// 单个 batch item 详情面板：标题 / 进度 / 审批 / 图片画廊 / Code Agent 日志 / 错误。
// 从 view.tsx 抽出，纯 props-driven，无内部状态除了本地 traceback 折叠开关。

import { useState } from "react";
import { AlertTriangle, ChevronDown, ChevronUp, RotateCcw } from "lucide-react";

import { AgentLog } from "../../components/AgentLog";
import { ApprovalPanel } from "../../components/ApprovalPanel";
import { StageStatus } from "../../components/StageStatus";
import type { PlanItem } from "../../lib/batch_ws";
import { cn } from "../../lib/utils";
import type { BatchItemState as ItemState } from "./state.ts";
import { STATUS_LABELS, TYPE_LABELS } from "./view-constants.ts";

export function ItemDetailPanel({
  item,
  state,
  onSelectImage,
  onGenerateMore,
  onRetryItem,
  approvalBusyActionId,
  onApproveAction,
  onRejectAction,
  onExecuteAction,
  onProceedApproval,
  proceedApprovalDisabled,
  onUpdatePrompt,
  onToggleMorePrompt,
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
        <span
          className={cn(
            "ml-auto text-xs px-2 py-0.5 rounded-full font-medium",
            state.status === "done"
              ? "bg-green-100 text-green-700"
              : state.status === "error"
                ? "bg-red-100 text-red-600"
                : state.status === "awaiting_selection"
                  ? "bg-violet-100 text-violet-700"
                  : state.status === "approval_pending"
                    ? "bg-violet-100 text-violet-700"
                    : state.status === "code_generating"
                      ? "bg-blue-100 text-blue-600"
                      : "bg-slate-100 text-slate-500",
          )}
        >
          {STATUS_LABELS[state.status]}
        </span>
      </div>

      {/* 进度日志 */}
      <StageStatus current={state.currentStage} history={state.stageHistory} isComplete={state.status === "done"} />
      {state.progress.length > 0 && <AgentLog lines={state.progress} />}

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
                  onChange={(e) => onUpdatePrompt(e.target.value)}
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
          <AgentLog lines={state.agentLog} entries={state.agentLogEntries} currentModel={state.currentAgentModel} />
        </div>
      )}

      {/* 完成提示 */}
      {state.status === "done" && <p className="text-sm text-green-600 font-medium">✓ {item.name} 创建完成</p>}

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
                onClick={() => setShowTrace((v) => !v)}
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
