import { useEffect, useState } from "react";
import { RefreshCcw } from "lucide-react";

import {
  loadPlatformQueueWorkerStatus,
  type PlatformQueueWorkerStatus,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminEventType } from "./adminDisplay.ts";

function formatTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未记录";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

function formatUnavailableReason(reason?: string): string {
  switch (reason) {
    case "not_platform_runtime":
      return "当前运行环境不是平台 Web 服务。";
    case "worker_not_configured":
      return "当前未配置平台队列 Worker。";
    default:
      return reason || "当前未暴露队列 Worker 运行态。";
  }
}

function RuntimeMetric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-lg border border-white bg-white/85 px-4 py-3 shadow-sm">
      <p className="text-xs font-semibold text-slate-500">{label}</p>
      <p className="mt-2 break-all text-lg font-semibold text-slate-900">{value}</p>
    </div>
  );
}

export function AdminRuntimePage() {
  const [status, setStatus] = useState<PlatformQueueWorkerStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadStatus() {
    setLoading(true);
    setError("");
    try {
      setStatus(await loadPlatformQueueWorkerStatus());
    } catch (loadError) {
      setStatus(null);
      setError(resolveErrorMessage(loadError) || "读取运行状态失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadStatus();
  }, []);

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">运行状态</h1>
          <p className="mt-1 text-sm text-slate-500">队列 Worker、调度权和最近运行事件。</p>
        </div>
        <button
          type="button"
          onClick={() => {
            void loadStatus();
          }}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
          disabled={loading}
        >
          <RefreshCcw size={16} />
          <span>{loading ? "刷新中" : "刷新状态"}</span>
        </button>
      </header>

      {error ? <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">{error}</section> : null}

      {!status?.available ? (
        <section className="rounded-lg border border-amber-100 bg-amber-50 px-4 py-3 text-sm text-amber-700">
          {formatUnavailableReason(status?.reason)}
        </section>
      ) : null}

      <section className="grid gap-3 md:grid-cols-4">
        <RuntimeMetric label="当前实例" value={status?.owner_id || "未记录"} />
        <RuntimeMetric label="当前角色" value={status?.is_leader ? "调度者" : "非调度者"} />
        <RuntimeMetric label="调度世代" value={status?.leader_epoch ?? "未记录"} />
        <RuntimeMetric label="最近 Tick" value={formatTime(status?.last_tick_at)} />
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">当前调度权</h2>
          {status?.current_leader ? (
            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <RuntimeMetric label="Owner" value={status.current_leader.owner_id} />
              <RuntimeMetric label="Epoch" value={status.current_leader.leader_epoch ?? "未记录"} />
              <RuntimeMetric label="获得时间" value={formatTime(status.current_leader.claimed_at)} />
              <RuntimeMetric label="续租时间" value={formatTime(status.current_leader.renewed_at)} />
              <RuntimeMetric label="过期时间" value={formatTime(status.current_leader.expires_at)} />
            </div>
          ) : (
            <p className="mt-3 text-sm text-slate-500">当前没有读取到有效的调度权租约。</p>
          )}
        </div>

        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <h2 className="text-base font-semibold text-slate-900">最近运行事件</h2>
          {status?.recent_leader_events?.length ? (
            <div className="mt-3 space-y-2">
              {[...status.recent_leader_events].slice(0, 8).map((event, index) => (
                <article key={`${event.event_type}-${event.occurred_at}-${index}`} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    <span className="font-medium text-slate-900">{formatAdminEventType(event.event_type)}</span>
                    <span className="text-xs text-slate-500">{formatTime(event.occurred_at)}</span>
                    <span className="text-xs text-slate-500">epoch {event.leader_epoch ?? "未记录"}</span>
                  </div>
                  {event.detail ? <p className="mt-1 text-xs text-slate-500">{event.detail}</p> : null}
                </article>
              ))}
            </div>
          ) : (
            <p className="mt-3 text-sm text-slate-500">当前还没有记录到运行事件。</p>
          )}
        </div>
      </section>
    </div>
  );
}

export default AdminRuntimePage;
