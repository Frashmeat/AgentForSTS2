import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { House } from "lucide-react";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { getMyJob, listMyJobItems } from "../../shared/api/me.ts";
import type { PlatformJobDetail, PlatformJobItemSummary } from "../../shared/api/platform.ts";
import { RefundSummary } from "./refundSummary.tsx";
import { useSession } from "../../shared/session/hooks.ts";

export function UserCenterJobDetailPage() {
  const { jobId } = useParams();
  const { isAuthenticated, isLoading } = useSession();
  const [detail, setDetail] = useState<PlatformJobDetail | null>(null);
  const [items, setItems] = useState<PlatformJobItemSummary[]>([]);
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
    void Promise.all([getMyJob(Number(jobId)), listMyJobItems(Number(jobId))])
      .then(([detailResult, itemResults]) => {
        if (!cancelled) {
          setDetail(detailResult);
          setItems(itemResults);
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

  return (
    <PlatformPageShell
      kicker="User Center"
      title={detail.input_summary || detail.job_type}
      description={`${detail.job_type} · ${detail.status}`}
      actions={navigationActions}
    >
      <RefundSummary detail={detail} />

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
    </PlatformPageShell>
  );
}

export default UserCenterJobDetailPage;
