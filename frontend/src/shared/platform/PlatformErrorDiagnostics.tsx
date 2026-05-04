import type { PlatformErrorView } from "./errors.ts";

interface PlatformErrorDiagnosticsProps {
  error: PlatformErrorView;
}

export function PlatformErrorDiagnostics({ error }: PlatformErrorDiagnosticsProps) {
  const rows = [
    ["错误来源", error.title],
    ["发生位置", runtimeSurfaceLabel(error.runtimeSurface)],
    ["组件", error.component],
    ["操作", error.operation],
    ["分类", error.category],
    ["诊断码", error.reasonCode],
    ["是否可重试", error.retryable === null ? "" : error.retryable ? "是" : "否"],
    ["失败步骤", [error.stepId, error.stepType].filter(Boolean).join(" / ")],
    ["模型", [error.provider, error.model].filter(Boolean).join(" / ")],
    ["HTTP 状态", error.httpStatus === null ? "" : String(error.httpStatus)],
    ["建议查看", [logHintLabel(error.logHint.primary), logHintLabel(error.logHint.secondary)].filter(Boolean).join(" / ")],
  ].filter(([, value]) => String(value || "").trim().length > 0);

  return (
    <section className="rounded-lg border border-slate-200 bg-white px-4 py-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-sm font-semibold text-slate-900">错误诊断</p>
          <p className="mt-1 text-sm text-slate-600">{error.message}</p>
        </div>
        <span className="rounded-md border border-slate-200 bg-slate-50 px-2 py-1 text-xs font-medium text-slate-700">
          {error.badgeLabel}
        </span>
      </div>
      <div className="mt-4 grid gap-2 sm:grid-cols-2">
        {rows.map(([label, value]) => (
          <div key={label} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
            <p className="text-[11px] font-semibold text-slate-400">{label}</p>
            <p className="mt-1 break-all text-xs text-slate-700">{value}</p>
          </div>
        ))}
      </div>
      {error.developerMessage ? (
        <div className="mt-3 rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
          <p className="text-[11px] font-semibold text-slate-400">开发诊断</p>
          <p className="mt-1 break-all text-xs leading-5 text-slate-700">{error.developerMessage}</p>
        </div>
      ) : null}
      {error.diagnosticRows.length > 0 ? (
        <details className="mt-3">
          <summary className="cursor-pointer text-xs font-medium text-slate-500">展开结构化诊断</summary>
          <div className="mt-2 grid gap-2 sm:grid-cols-2">
            {error.diagnosticRows.map((row) => (
              <div key={row.label} className="rounded-lg border border-slate-100 bg-white px-3 py-2">
                <p className="text-[11px] font-semibold text-slate-400">{row.label}</p>
                <p className="mt-1 break-all text-xs leading-5 text-slate-600">{row.value}</p>
              </div>
            ))}
          </div>
        </details>
      ) : null}
    </section>
  );
}

function runtimeSurfaceLabel(value: string): string {
  switch (value) {
    case "web":
      return "Web 后端";
    case "web_workstation":
      return "Web 工作站";
    case "local_workstation":
      return "本机工作站";
    case "browser":
      return "浏览器";
    default:
      return "";
  }
}

function logHintLabel(value: string): string {
  switch (value) {
    case "web_backend_log":
      return "Web 后端日志";
    case "web_workstation_stderr":
      return "Web Workstation stderr";
    case "web_workstation_stdout":
      return "Web Workstation stdout";
    case "web_workstation_log":
      return "Web Workstation 日志";
    case "web_log":
      return "Web 日志";
    case "local_workstation_log":
      return "本机 Workstation 日志";
    default:
      return value;
  }
}
