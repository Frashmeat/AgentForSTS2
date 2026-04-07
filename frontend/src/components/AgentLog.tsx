import { useEffect, useRef } from "react";
import { cn } from "../lib/utils";
import {
  buildPrettyWorkflowLogLines,
  buildRawWorkflowLogLines,
  type WorkflowLogEntry,
} from "../shared/workflowLog.ts";
import { useMemo, useState } from "react";

interface Props {
  lines?: string[];
  entries?: WorkflowLogEntry[];
  currentModel?: string | null;
  className?: string;
}

export function AgentLog({ lines = [], entries, currentModel, className }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [activeView, setActiveView] = useState<"pretty" | "raw">("pretty");
  const normalizedEntries = useMemo(
    () => entries ?? lines.map((line) => ({ text: line })),
    [entries, lines],
  );
  const renderedLines = useMemo(
    () => activeView === "pretty"
      ? buildPrettyWorkflowLogLines(normalizedEntries)
      : buildRawWorkflowLogLines(normalizedEntries),
    [activeView, normalizedEntries],
  );

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [renderedLines]);

  return (
    <div
      className={cn(
        "bg-slate-50 border border-slate-200 rounded-lg overflow-hidden",
        className
      )}
    >
      <div className="flex items-center justify-between gap-3 border-b border-slate-200 bg-white px-3 py-2">
        <div className="min-w-0">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-slate-400">当前模型</p>
          <p className="truncate font-mono text-[11px] text-slate-600">{currentModel?.trim() || "当前阶段未调用模型"}</p>
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
            优化后输出
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
      </div>
      <div className="max-h-64 overflow-y-auto p-3 font-mono text-xs text-slate-600">
        {renderedLines.map((line, i) => (
          <div key={`${activeView}-${i}`} className="whitespace-pre-wrap leading-5">{line}</div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
