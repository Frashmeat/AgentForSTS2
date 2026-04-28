import { useEffect, useMemo, useState } from "react";
import { RefreshCcw, ShieldAlert } from "lucide-react";

import { listAdminAuditEvents, type AdminAuditEvent } from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { useSession } from "../../shared/session/hooks.ts";
import { formatAdminEventType } from "./adminDisplay.ts";

const DEFAULT_AUDIT_LIMIT = 50;

function formatTime(value: string): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未记录";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

function toFriendlyAuditError(error: unknown): string {
  const message = resolveErrorMessage(error);
  if (message.includes("admin permission required")) {
    return "当前账号没有管理员权限，无法查看审计事件。";
  }
  if (message.includes("authentication required")) {
    return "当前登录状态已失效，请重新登录后再查看审计事件。";
  }
  return message || "读取审计事件失败";
}

export function AdminAuditPage() {
  const { isAuthenticated, isLoading, refreshSession } = useSession();
  const [events, setEvents] = useState<AdminAuditEvent[]>([]);
  const [filterMode, setFilterMode] = useState<"all" | "queue_worker">("queue_worker");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadEvents() {
    if (!isAuthenticated) {
      return;
    }
    setLoading(true);
    setError("");
    try {
      const result = await listAdminAuditEvents(
        undefined,
        filterMode === "queue_worker" ? "runtime.queue_worker." : undefined,
        undefined,
        DEFAULT_AUDIT_LIMIT,
      );
      setEvents([...result].sort((left, right) => new Date(right.occurred_at).getTime() - new Date(left.occurred_at).getTime()));
    } catch (loadError) {
      const message = toFriendlyAuditError(loadError);
      if (message.includes("重新登录")) {
        void refreshSession();
      }
      setError(message);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadEvents();
  }, [isAuthenticated, filterMode]);

  const eventTypeSummary = useMemo(() => {
    const counts = events.reduce<Record<string, number>>((accumulator, event) => {
      const label = formatAdminEventType(event.event_type);
      accumulator[label] = (accumulator[label] ?? 0) + 1;
      return accumulator;
    }, {});
    return Object.entries(counts).sort((left, right) => right[1] - left[1]);
  }, [events]);

  if (isLoading) {
    return <section className="rounded-lg border border-white bg-white/80 p-6 text-sm text-slate-500">正在恢复管理员会话...</section>;
  }

  if (!isAuthenticated) {
    return (
      <section className="rounded-lg border border-white bg-white/80 p-6">
        <h1 className="text-xl font-semibold text-slate-950">审计事件</h1>
        <p className="mt-2 text-sm text-slate-500">登录后查看管理员审计事件。</p>
      </section>
    );
  }

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">审计事件</h1>
          <p className="mt-1 text-sm text-slate-500">事件流查询与运行时行为排查。</p>
        </div>
        <button
          type="button"
          onClick={() => {
            void loadEvents();
          }}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
          disabled={loading}
        >
          <RefreshCcw size={16} />
          <span>{loading ? "刷新中" : "刷新事件"}</span>
        </button>
      </header>

      {error ? <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">{error}</section> : null}

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="inline-flex gap-2">
            <button
              type="button"
              onClick={() => setFilterMode("queue_worker")}
              className={`rounded-lg border px-3 py-1.5 text-xs transition ${
                filterMode === "queue_worker"
                  ? "border-violet-200 bg-violet-50 text-violet-700"
                  : "border-slate-200 text-slate-500 hover:border-violet-200 hover:text-violet-700"
              }`}
            >
              仅队列 Worker
            </button>
            <button
              type="button"
              onClick={() => setFilterMode("all")}
              className={`rounded-lg border px-3 py-1.5 text-xs transition ${
                filterMode === "all"
                  ? "border-violet-200 bg-violet-50 text-violet-700"
                  : "border-slate-200 text-slate-500 hover:border-violet-200 hover:text-violet-700"
              }`}
            >
              全部事件
            </button>
          </div>
          <div className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 px-3 py-1.5 text-xs text-slate-600">
            <ShieldAlert size={14} />
            <span>{events.length} 条事件</span>
          </div>
        </div>

        {eventTypeSummary.length > 0 ? (
          <div className="mt-4 flex flex-wrap gap-2">
            {eventTypeSummary.map(([label, count]) => (
              <span key={label} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-1.5 text-xs text-slate-600">
                {label} · {count}
              </span>
            ))}
          </div>
        ) : null}
      </section>

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        {loading && events.length === 0 ? (
          <p className="text-sm text-slate-500">正在读取审计事件...</p>
        ) : events.length === 0 ? (
          <p className="text-sm text-slate-500">当前还没有审计事件。</p>
        ) : (
          <div className="space-y-3">
            {events.map((event) => (
              <article key={`${event.event_id}-${event.occurred_at}`} className="rounded-lg border border-slate-100 bg-slate-50 px-4 py-3">
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <span className="rounded-md bg-white px-2.5 py-1 text-xs font-semibold text-slate-700">
                    {formatAdminEventType(event.event_type)}
                  </span>
                  <span className="text-xs text-slate-500">{formatTime(event.occurred_at)}</span>
                  <span className="text-xs text-slate-500">事件编号 {event.event_id}</span>
                  <span className="text-xs text-slate-500">任务编号 {event.job_id}</span>
                  {typeof event.job_item_id === "number" ? <span className="text-xs text-slate-500">子任务 {event.job_item_id}</span> : null}
                </div>
                <details className="mt-3">
                  <summary className="cursor-pointer text-xs font-medium text-slate-500">技术信息</summary>
                  <div className="mt-2 grid gap-2 sm:grid-cols-2">
                    {Object.entries(event.payload ?? {}).map(([key, value]) => (
                      <div key={key} className="rounded-lg border border-slate-100 bg-white px-3 py-2">
                        <p className="text-[11px] font-semibold text-slate-400">{key}</p>
                        <p className="mt-1 break-all text-xs text-slate-600">
                          {typeof value === "string" ? value : JSON.stringify(value)}
                        </p>
                      </div>
                    ))}
                  </div>
                </details>
              </article>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

export default AdminAuditPage;
