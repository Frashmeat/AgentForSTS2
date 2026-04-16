import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { AlertTriangle, Clock3, House } from "lucide-react";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { getMyJob, listMyJobEvents, listMyJobItems } from "../../shared/api/me.ts";
import type {
  PlatformJobDetail,
  PlatformJobEventSummary,
  PlatformJobItemSummary,
} from "../../shared/api/platform.ts";
import { readDeferredExecutionNotice } from "../../shared/deferredExecution.ts";
import { RefundSummary } from "./refundSummary.tsx";
import { useSession } from "../../shared/session/hooks.ts";

function formatOccurredAt(value: string) {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString("zh-CN", { hour12: false });
}

export function UserCenterJobDetailPage() {
  const { jobId } = useParams();
  const { isAuthenticated, isLoading } = useSession();
  const [detail, setDetail] = useState<PlatformJobDetail | null>(null);
  const [items, setItems] = useState<PlatformJobItemSummary[]>([]);
  const [events, setEvents] = useState<PlatformJobEventSummary[]>([]);
  const [error, setError] = useState("");
  const navigationActions = (
    <>
      <Link to="/me" className="platform-page-action-link">
        返回用户中心
      </Link>
      <Link to="/" className="platform-page-action-link">
        <House size={16} />
        <span>返回首页</span>
      </Link>
    </>
  );

  useEffect(() => {
    if (!isAuthenticated || !jobId) {
      return;
    }
    let cancelled = false;
    void Promise.all([getMyJob(Number(jobId)), listMyJobItems(Number(jobId)), listMyJobEvents(Number(jobId))])
      .then(([detailResult, itemResults, eventResults]) => {
        if (!cancelled) {
          setDetail(detailResult);
          setItems(itemResults);
          setEvents(eventResults);
        }
      })
      .catch(err => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "加载任务详情失败");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [isAuthenticated, jobId]);

  if (isLoading) {
    return (
      <PlatformPageShell kicker="User Center" title="任务详情" description="正在恢复会话并读取平台任务详情。" actions={navigationActions}>
        <section className="platform-page-card p-8 text-sm text-slate-500">正在恢复会话...</section>
      </PlatformPageShell>
    );
  }

  if (!isAuthenticated) {
    return (
      <PlatformPageShell kicker="User Center" title="任务详情" description="登录后可查看服务器模式任务的执行结果与返还信息。" actions={navigationActions}>
        <section className="platform-page-card p-8">
          <Link to="/auth/login" className="text-sm font-medium text-[var(--workspace-accent)] transition hover:text-[var(--workspace-accent-strong)]">
            登录后查看任务详情
          </Link>
        </section>
      </PlatformPageShell>
    );
  }

  if (error) {
    return (
      <PlatformPageShell kicker="User Center" title="任务详情" description="平台任务详情加载失败。" actions={navigationActions}>
        <section className="platform-page-card p-8 text-sm text-rose-600">{error}</section>
      </PlatformPageShell>
    );
  }

  if (detail === null) {
    return (
      <PlatformPageShell kicker="User Center" title="任务详情" description="正在读取任务状态、返还摘要与子项列表。" actions={navigationActions}>
        <section className="platform-page-card p-8 text-sm text-slate-500">任务详情加载中...</section>
      </PlatformPageShell>
    );
  }

  const deferredNotice = readDeferredExecutionNotice(events);

  return (
    <PlatformPageShell
      kicker="User Center"
      title={detail.input_summary || detail.job_type}
      description={`${detail.job_type} · ${detail.status}`}
      actions={navigationActions}
    >
      <RefundSummary detail={detail} />

      {deferredNotice ? (
        <section className="platform-page-card p-6">
          <div className="flex items-start gap-3 rounded-[22px] border border-amber-200 bg-amber-50/90 px-4 py-4 text-amber-950">
            <AlertTriangle size={18} className="mt-0.5 shrink-0 text-amber-600" />
            <div className="space-y-2">
              <div>
                <p className="text-sm font-semibold">{deferredNotice.summary.title}</p>
                <p className="mt-1 text-sm text-amber-900/80">{deferredNotice.summary.description}</p>
              </div>
              <div className="text-xs text-amber-900/70">
                <p>事件时间：{formatOccurredAt(deferredNotice.event.occurred_at)}</p>
                <p>后端说明：{deferredNotice.summary.detail}</p>
              </div>
            </div>
          </div>
        </section>
      ) : null}

      <section className="platform-page-card p-6">
        <h2 className="text-lg font-semibold text-slate-900">子项列表</h2>
        <div className="mt-6 space-y-3">
          {items.map(item => (
            <article key={item.id} className="platform-page-subcard px-4 py-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold text-slate-900">
                    #{item.item_index + 1} · {item.item_type}
                  </p>
                  <p className="mt-1 text-xs text-slate-500">{item.status}</p>
                </div>
                <div className="max-w-xs text-right text-xs text-slate-500">
                  <p>{item.result_summary || "无结果摘要"}</p>
                  <p>{item.error_summary || "无错误信息"}</p>
                </div>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section className="platform-page-card p-6">
        <div className="flex items-center gap-2">
          <Clock3 size={16} className="text-[var(--workspace-accent)]" />
          <h2 className="text-lg font-semibold text-slate-900">执行事件</h2>
        </div>
        <div className="mt-6 space-y-3">
          {events.length === 0 ? (
            <div className="platform-page-empty px-4 py-6 text-sm text-slate-500">当前任务还没有可展示的执行事件。</div>
          ) : (
            events.map(event => (
              <article key={event.event_id} className="platform-page-subcard px-4 py-4">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <p className="text-sm font-semibold text-slate-900">{event.event_type}</p>
                    <p className="mt-1 text-xs text-slate-500">{formatOccurredAt(event.occurred_at)}</p>
                  </div>
                  <div className="max-w-xl text-right text-xs text-slate-500">
                    {event.event_type === "ai_execution.deferred" ? (
                      <p>{String(event.payload.reason_message ?? "当前任务尚未进入真实服务器执行。")}</p>
                    ) : (
                      <p>{JSON.stringify(event.payload)}</p>
                    )}
                  </div>
                </div>
              </article>
            ))
          )}
        </div>
      </section>
    </PlatformPageShell>
  );
}

export default UserCenterJobDetailPage;
