// SettingsPanel 内部用的小型布局子组件：进度条 / 分组容器 / 字段标签。

import type React from "react";

export function ProgressBar({
  label,
  progress,
  tone = "amber",
  indeterminate = false,
}: {
  label: string;
  progress?: number;
  tone?: "amber" | "sky";
  indeterminate?: boolean;
}) {
  const trackCls = tone === "sky" ? "bg-sky-100" : "bg-amber-100";
  const fillCls = tone === "sky" ? "bg-sky-500" : "bg-amber-500";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-3">
        <p className="text-[11px] font-medium text-slate-600">{label}</p>
        {!indeterminate && typeof progress === "number" ? (
          <span className="text-[11px] font-semibold text-slate-500">{Math.round(progress)}%</span>
        ) : null}
      </div>
      <div className={`h-2 overflow-hidden rounded-full ${trackCls}`}>
        {indeterminate ? (
          <div className="h-full w-1/3 rounded-full bg-sky-500 animate-[knowledge-progress-indeterminate_1.2s_ease-in-out_infinite]" />
        ) : (
          <div
            className={`h-full rounded-full ${fillCls} transition-[width] duration-300 ease-out`}
            style={{ width: `${Math.max(0, Math.min(progress ?? 0, 100))}%` }}
          />
        )}
      </div>
    </div>
  );
}

export function SGroup({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-slate-400">{icon}</span>
        <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider">{title}</h3>
      </div>
      <div className="space-y-2.5 pl-1">{children}</div>
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <label className="text-xs font-medium text-slate-500">{label}</label>
      {children}
      {hint && <p className="text-xs text-slate-400">{hint}</p>}
    </div>
  );
}
