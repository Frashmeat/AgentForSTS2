import { AlertTriangle, RefreshCw, Settings } from "lucide-react";

import type { PlatformErrorView } from "./errors.ts";

interface PlatformErrorSummaryProps {
  error: PlatformErrorView;
  onRetry?: () => void;
  onOpenSettings?: () => void;
}

export function PlatformErrorSummary({ error, onRetry, onOpenSettings }: PlatformErrorSummaryProps) {
  const toneClass =
    error.badgeTone === "warning"
      ? "border-amber-200 bg-amber-50 text-amber-950"
      : error.badgeTone === "info"
        ? "border-sky-200 bg-sky-50 text-sky-950"
        : "border-rose-200 bg-rose-50 text-rose-950";

  return (
    <section className={`rounded-lg border px-4 py-3 ${toneClass}`}>
      <div className="flex items-start gap-3">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
        <div className="min-w-0 flex-1 space-y-2">
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-sm font-semibold">{error.title}</p>
              <span className="rounded-md border border-current/20 px-2 py-0.5 text-[11px] font-medium">
                {error.badgeLabel}
              </span>
            </div>
            <p className="mt-1 text-sm leading-5 opacity-90">{error.message}</p>
          </div>
          <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs opacity-80">
            {error.reasonCode ? <span>诊断码：{error.reasonCode}</span> : null}
            {error.stepId ? <span>步骤：{error.stepId}</span> : null}
            {error.retryable !== null ? <span>可重试：{error.retryable ? "是" : "否"}</span> : null}
          </div>
          {onRetry || onOpenSettings ? (
            <div className="flex flex-wrap gap-2 pt-1">
              {onRetry && error.retryable ? (
                <button
                  type="button"
                  onClick={onRetry}
                  className="inline-flex items-center gap-1 rounded-md bg-white/80 px-2.5 py-1.5 text-xs font-semibold text-slate-800 shadow-sm transition hover:bg-white"
                >
                  <RefreshCw size={13} />
                  <span>重试</span>
                </button>
              ) : null}
              {onOpenSettings && error.origin === "upstream" ? (
                <button
                  type="button"
                  onClick={onOpenSettings}
                  className="inline-flex items-center gap-1 rounded-md bg-white/80 px-2.5 py-1.5 text-xs font-semibold text-slate-800 shadow-sm transition hover:bg-white"
                >
                  <Settings size={13} />
                  <span>切换执行配置</span>
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}
