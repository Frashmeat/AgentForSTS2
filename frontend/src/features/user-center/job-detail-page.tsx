import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
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
    return <div className="px-6 py-10 text-sm text-slate-500">正在恢复会话...</div>;
  }

  if (!isAuthenticated) {
    return (
      <div className="px-6 py-10">
        <Link to="/auth/login" className="text-sm text-amber-600 hover:text-amber-700">
          登录后查看任务详情
        </Link>
      </div>
    );
  }

  if (error) {
    return <div className="px-6 py-10 text-sm text-rose-600">{error}</div>;
  }

  if (detail === null) {
    return <div className="px-6 py-10 text-sm text-slate-500">任务详情加载中...</div>;
  }

  return (
    <div className="px-6 py-8">
      <div className="mx-auto max-w-6xl space-y-6">
        <section className="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <Link to="/me" className="text-sm text-amber-600 hover:text-amber-700">
            返回用户中心
          </Link>
          <h1 className="mt-3 text-3xl font-semibold text-slate-900">{detail.input_summary || detail.job_type}</h1>
          <p className="mt-2 text-sm text-slate-500">
            {detail.job_type} · {detail.status}
          </p>
        </section>

        <RefundSummary detail={detail} />

        <section className="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <h2 className="text-lg font-semibold text-slate-900">子项列表</h2>
          <div className="mt-6 space-y-3">
            {items.map(item => (
              <article key={item.id} className="rounded-2xl border border-slate-200 px-4 py-4">
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
      </div>
    </div>
  );
}

export default UserCenterJobDetailPage;
