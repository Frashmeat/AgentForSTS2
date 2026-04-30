import { useEffect, useState } from "react";
import { ArrowLeft, FileSearch, KeyRound, ReceiptText, RefreshCcw, ShieldAlert } from "lucide-react";
import { Link } from "react-router-dom";

import { PlatformPageShell } from "../components/platform/PlatformPageShell.tsx";
import { listAdminAuditEvents, type AdminAuditEvent } from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";
import { useSession } from "../shared/session/hooks.ts";

const DEFAULT_AUDIT_LIMIT = 50;

function formatAuditTime(value: string): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "—";
  }
  const date = new Date(text);
  if (Number.isNaN(date.getTime())) {
    return text;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}

function formatAuditLabel(eventType: string): string {
  switch (eventType) {
    case "runtime.queue_worker.leader_acquired":
      return "成为 Leader";
    case "runtime.queue_worker.leader_taken_over":
      return "接管 Leader";
    case "runtime.queue_worker.leader_observed_other":
      return "观察到其他 Leader";
    case "runtime.queue_worker.leader_lost":
      return "失去 Leader";
    case "runtime.queue_worker.leader_released":
      return "主动释放 Leader";
    case "runtime.queue_worker.leader_waiting_for_failover":
      return "等待 Failover";
    default:
      return eventType;
  }
}

function toFriendlyAuditError(error: unknown): string {
  const message = resolveErrorMessage(error);
  if (message.includes("admin permission required")) {
    return "当前账号没有管理员权限，无法查看运行时审计事件。";
  }
  if (message.includes("authentication required")) {
    return "当前登录状态已失效，请重新登录后再查看管理员审计事件。";
  }
  return message || "读取管理员审计事件失败";
}

export function AdminRuntimeAuditPage() {
  const { isAuthenticated, isLoading, refreshSession } = useSession();
  const [events, setEvents] = useState<AdminAuditEvent[]>([]);
  const [filterMode, setFilterMode] = useState<"all" | "queue_worker">("queue_worker");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadEvents() {
    setLoading(true);
    setError("");
    try {
      const result = await listAdminAuditEvents(
        undefined,
        filterMode === "queue_worker" ? "runtime.queue_worker." : undefined,
        undefined,
        DEFAULT_AUDIT_LIMIT,
      );
      const merged = [...result].sort((left, right) => {
        const leftTime = new Date(left.occurred_at).getTime();
        const rightTime = new Date(right.occurred_at).getTime();
        return rightTime - leftTime;
      });
      setEvents(merged);
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
    if (!isAuthenticated) {
      return;
    }
    void loadEvents();
    // loadEvents 是组件内闭包，每次 render 引用变化；deps 只列触发刷新的 state。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isAuthenticated, filterMode]);

  const backAction = (
    <Link to="/settings" className="platform-page-action-link">
      <ArrowLeft size={16} />
      <span>返回设置</span>
    </Link>
  );
  const credentialsAction = (
    <Link to="/admin/server-credentials" className="platform-page-action-link">
      <KeyRound size={16} />
      <span>凭据管理</span>
    </Link>
  );
  const executionsAction = (
    <Link to="/admin/executions" className="platform-page-action-link">
      <FileSearch size={16} />
      <span>执行记录</span>
    </Link>
  );
  const refundsAction = (
    <Link to="/admin/refunds" className="platform-page-action-link">
      <ReceiptText size={16} />
      <span>退款记录</span>
    </Link>
  );

  const filteredEvents = events;

  const eventTypeCounts = filteredEvents.reduce<Record<string, number>>((counts, event) => {
    const key = formatAuditLabel(event.event_type);
    counts[key] = (counts[key] ?? 0) + 1;
    return counts;
  }, {});

  const eventTypeSummary = Object.entries(eventTypeCounts).sort((left, right) => right[1] - left[1]);

  if (isLoading) {
    return (
      <PlatformPageShell
        kicker="Admin Runtime Audit"
        title="运行时审计"
        description="正在恢复管理员会话状态。"
        actions={backAction}
      >
        <section className="platform-page-card p-8 text-sm text-slate-500">正在恢复会话...</section>
      </PlatformPageShell>
    );
  }

  if (!isAuthenticated) {
    return (
      <PlatformPageShell
        kicker="Admin Runtime Audit"
        title="运行时审计"
        description="登录后才能读取管理员运行时审计事件。"
        actions={backAction}
      >
        <section className="platform-page-card p-8 space-y-4">
          <p className="text-sm text-slate-500">当前未登录，无法读取管理员审计事件。</p>
          <Link to="/auth/login" className="platform-page-primary-button inline-flex">
            去登录
          </Link>
        </section>
      </PlatformPageShell>
    );
  }

  return (
    <PlatformPageShell
      kicker="Admin Runtime Audit"
      title="运行时审计"
      description="这里展示平台运行时的审计事件，包括 queue worker leader 切换、接管与退避等系统级行为。"
      actions={
        <div className="flex flex-wrap gap-2">
          {backAction}
          {credentialsAction}
          {executionsAction}
          {refundsAction}
          <button
            type="button"
            onClick={() => {
              void loadEvents();
            }}
            className="platform-page-action-link"
          >
            <RefreshCcw size={16} />
            <span>刷新事件</span>
          </button>
        </div>
      }
    >
      {error ? <section className="platform-page-card p-6 text-sm text-rose-600">{error}</section> : null}

      <section className="platform-page-card p-6 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold text-slate-900">最近审计事件</h2>
            <p className="text-sm text-slate-500">
              优先展示最新事件；`job_id = 0` 的事件表示运行时系统事件，而不是具体平台任务。
            </p>
          </div>
          <div className="inline-flex items-center gap-2 rounded-full border border-slate-200 bg-slate-50 px-3 py-1 text-xs text-slate-600">
            <ShieldAlert size={14} />
            <span>{loading ? "刷新中…" : `${events.length} 条事件`}</span>
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => setFilterMode("queue_worker")}
            className={`rounded-full border px-3 py-1.5 text-xs transition-colors ${
              filterMode === "queue_worker"
                ? "border-amber-300 bg-amber-50 text-amber-700"
                : "border-slate-200 text-slate-500 hover:border-amber-200 hover:text-amber-700"
            }`}
          >
            仅 queue worker
          </button>
          <button
            type="button"
            onClick={() => setFilterMode("all")}
            className={`rounded-full border px-3 py-1.5 text-xs transition-colors ${
              filterMode === "all"
                ? "border-amber-300 bg-amber-50 text-amber-700"
                : "border-slate-200 text-slate-500 hover:border-amber-200 hover:text-amber-700"
            }`}
          >
            全部事件
          </button>
        </div>

        {eventTypeSummary.length > 0 ? (
          <div className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3">
            <p className="text-xs font-semibold uppercase tracking-wider text-slate-500">事件类型统计</p>
            <div className="mt-3 flex flex-wrap gap-2">
              {eventTypeSummary.map(([label, count]) => (
                <div
                  key={label}
                  className="inline-flex items-center gap-2 rounded-full border border-slate-200 bg-white px-3 py-1.5 text-xs text-slate-600"
                >
                  <span>{label}</span>
                  <span className="font-semibold text-slate-900">{count}</span>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {loading ? (
          <p className="text-sm text-slate-500">正在读取管理员审计事件…</p>
        ) : filteredEvents.length === 0 ? (
          <p className="text-sm text-slate-500">当前还没有审计事件。</p>
        ) : (
          <div className="space-y-3">
            {filteredEvents.map((event) => (
              <article
                key={`${event.event_id}-${event.occurred_at}`}
                className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3"
              >
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <span className="rounded-full bg-white px-2.5 py-1 text-xs font-semibold text-slate-700">
                    {formatAuditLabel(event.event_type)}
                  </span>
                  <span className="text-xs text-slate-500">{formatAuditTime(event.occurred_at)}</span>
                  <span className="text-xs text-slate-500">event #{event.event_id}</span>
                  <span className="text-xs text-slate-500">job #{event.job_id}</span>
                  {typeof event.job_item_id === "number" ? (
                    <span className="text-xs text-slate-500">item #{event.job_item_id}</span>
                  ) : null}
                </div>
                <div className="mt-3 grid gap-2 sm:grid-cols-2">
                  {Object.entries(event.payload ?? {}).map(([key, value]) => (
                    <div key={key} className="rounded-lg border border-slate-100 bg-white px-3 py-2">
                      <p className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">{key}</p>
                      <p className="mt-1 break-all text-xs text-slate-600">
                        {typeof value === "string" ? value : JSON.stringify(value)}
                      </p>
                    </div>
                  ))}
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </PlatformPageShell>
  );
}

export default AdminRuntimeAuditPage;
