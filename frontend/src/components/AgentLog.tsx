import { useEffect, useRef } from "react";
import { cn } from "../lib/utils";
import {
  buildCodegenBroadcastView,
  buildPrettyWorkflowLogLines,
  buildRawWorkflowLogLines,
  type WorkflowLogEntry,
} from "../shared/workflowLog.ts";
import { useMemo, useState } from "react";

interface Props {
  lines?: string[];
  entries?: WorkflowLogEntry[];
  currentModel?: string | null;
  currentStage?: string | null;
  isComplete?: boolean;
  broadcastKind?: "codegen";
  className?: string;
}

export function AgentLog({
  lines = [],
  entries,
  currentModel,
  currentStage,
  isComplete = false,
  broadcastKind,
  className,
}: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [activeView, setActiveView] = useState<"pretty" | "raw">("pretty");
  const [detailExpanded, setDetailExpanded] = useState(false);
  const normalizedEntries = useMemo(() => entries ?? lines.map((line) => ({ text: line })), [entries, lines]);
  const codegenBroadcast = useMemo(
    () =>
      broadcastKind === "codegen" ? buildCodegenBroadcastView(normalizedEntries, { currentStage, isComplete }) : null,
    [broadcastKind, currentStage, isComplete, normalizedEntries],
  );
  const renderedLines = useMemo(
    () =>
      activeView === "pretty"
        ? buildPrettyWorkflowLogLines(normalizedEntries)
        : buildRawWorkflowLogLines(normalizedEntries),
    [activeView, normalizedEntries],
  );

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [activeView, codegenBroadcast, renderedLines]);

  return (
    <div className={cn("bg-slate-50 border border-slate-200 rounded-lg overflow-hidden", className)}>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-200 bg-white px-3 py-2">
        <div className="min-w-0">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-slate-400">当前模型</p>
          <p className="truncate font-mono text-[11px] text-slate-600">
            {currentModel?.trim() || "当前阶段未调用模型"}
          </p>
        </div>
        <div className="flex items-center gap-1 rounded-lg border border-slate-200 bg-slate-50 p-0.5 text-[11px]">
          <button
            type="button"
            onClick={() => setActiveView("pretty")}
            className={cn(
              "rounded-md px-2 py-1 transition-colors",
              activeView === "pretty" ? "bg-violet-600 text-white" : "text-slate-500 hover:text-slate-700",
            )}
          >
            {broadcastKind === "codegen" ? "执行播报" : "优化后输出"}
          </button>
          <button
            type="button"
            onClick={() => setActiveView("raw")}
            className={cn(
              "rounded-md px-2 py-1 transition-colors",
              activeView === "raw" ? "bg-violet-600 text-white" : "text-slate-500 hover:text-slate-700",
            )}
          >
            原始输出
          </button>
        </div>
        {codegenBroadcast && (
          <div className="rounded-lg border border-violet-200 bg-violet-50 px-2.5 py-1 text-[11px] font-semibold text-violet-700">
            {codegenBroadcast.progressLabel}
          </div>
        )}
      </div>
      {activeView === "pretty" && codegenBroadcast ? (
        <div className="max-h-[28rem] overflow-y-auto bg-[linear-gradient(180deg,#ffffff_0%,#f8fafc_100%)] p-4 text-sm text-slate-700">
          <div className="space-y-4">
            <section className="rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm">
              <p className="text-[11px] font-semibold uppercase tracking-wide text-slate-400">当前状态</p>
              <p className="mt-2 leading-6 text-slate-700">{codegenBroadcast.currentStatus}</p>
            </section>

            <section className="rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm">
              <div className="flex items-center justify-between gap-3">
                <p className="text-[11px] font-semibold uppercase tracking-wide text-slate-400">代码生成进度</p>
                <span className="rounded-full bg-slate-100 px-2 py-0.5 text-[11px] font-medium text-slate-500">
                  {codegenBroadcast.currentStep.index}. {codegenBroadcast.currentStep.label}
                </span>
              </div>
              <div className="mt-3 space-y-2">
                {codegenBroadcast.steps.map((step) => (
                  <div key={step.index} className="flex items-center gap-3">
                    <span
                      className={cn(
                        "flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-[11px] font-semibold",
                        step.status === "done" && "border-emerald-200 bg-emerald-50 text-emerald-700",
                        step.status === "current" && "border-violet-200 bg-violet-50 text-violet-700",
                        step.status === "pending" && "border-slate-200 bg-slate-50 text-slate-400",
                      )}
                    >
                      {step.status === "done" ? "✓" : step.status === "current" ? "●" : "○"}
                    </span>
                    <p className={cn("text-sm", step.status === "pending" ? "text-slate-400" : "text-slate-700")}>
                      {step.index}. {step.label}
                    </p>
                  </div>
                ))}
              </div>
            </section>

            <section className="rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm">
              <p className="text-[11px] font-semibold uppercase tracking-wide text-slate-400">执行摘要</p>
              <div className="mt-3 space-y-2">
                {codegenBroadcast.summaryLines.map((line, index) => (
                  <p key={index} className="leading-6 text-slate-700">
                    - {line}
                  </p>
                ))}
              </div>
            </section>

            <section className="rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm">
              <button
                type="button"
                onClick={() => setDetailExpanded((value) => !value)}
                className="flex w-full items-center justify-between gap-3 text-left"
              >
                <span className="text-[11px] font-semibold uppercase tracking-wide text-slate-400">技术细节</span>
                <span className="text-xs font-medium text-slate-500">{detailExpanded ? "收起" : "展开"}</span>
              </button>
              {detailExpanded && (
                <div className="mt-3 space-y-2 rounded-lg bg-slate-50 p-3 font-mono text-xs text-slate-600">
                  {(codegenBroadcast.detailLines.length > 0
                    ? codegenBroadcast.detailLines
                    : ["当前阶段暂无额外技术细节。"]
                  ).map((line, index) => (
                    <div key={index} className="whitespace-pre-wrap leading-5">
                      {line}
                    </div>
                  ))}
                </div>
              )}
            </section>
          </div>
          <div ref={bottomRef} />
        </div>
      ) : (
        <div className="max-h-64 overflow-y-auto p-3 font-mono text-xs text-slate-600">
          {renderedLines.map((line, i) => (
            <div key={`${activeView}-${i}`} className="whitespace-pre-wrap leading-5">
              {line}
            </div>
          ))}
          <div ref={bottomRef} />
        </div>
      )}
    </div>
  );
}
