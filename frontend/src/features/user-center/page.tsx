import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { HistoryList } from "./historyList.tsx";
import { loadUserCenterOverview, type UserCenterOverview } from "./model.ts";
import { QuotaCard } from "./quotaCard.tsx";
import { useSession } from "../../shared/session/hooks.ts";

export function UserCenterPage() {
  const { isAuthenticated, isLoading } = useSession();
  const [overview, setOverview] = useState<UserCenterOverview | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!isAuthenticated) {
      return;
    }
    let cancelled = false;
    void loadUserCenterOverview()
      .then(result => {
        if (!cancelled) {
          setOverview(result);
        }
      })
      .catch(err => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "加载用户中心失败");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [isAuthenticated]);

  if (isLoading) {
    return <div className="px-6 py-10 text-sm text-slate-500">正在恢复会话...</div>;
  }

  if (!isAuthenticated) {
    return (
      <div className="px-6 py-10">
        <section className="mx-auto max-w-3xl rounded-3xl border border-slate-200 bg-white p-8 shadow-sm">
          <h1 className="text-2xl font-semibold text-slate-900">登录后查看用户中心</h1>
          <p className="mt-2 text-sm text-slate-500">平台任务、统一次数池和返还记录都绑定当前账号。</p>
          <Link
            to="/auth/login"
            className="mt-6 inline-flex rounded-xl bg-amber-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-amber-600"
          >
            去登录
          </Link>
        </section>
      </div>
    );
  }

  if (error) {
    return <div className="px-6 py-10 text-sm text-rose-600">{error}</div>;
  }

  if (overview === null) {
    return <div className="px-6 py-10 text-sm text-slate-500">加载用户中心中...</div>;
  }

  return (
    <div className="px-6 py-8">
      <div className="mx-auto max-w-6xl space-y-6">
        <section className="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <p className="text-sm uppercase tracking-[0.24em] text-slate-400">User Center</p>
          <h1 className="mt-2 text-3xl font-semibold text-slate-900">{overview.profile.username}</h1>
          <p className="mt-2 text-sm text-slate-500">
            {overview.profile.email}
            {overview.profile.email_verified ? " · 邮箱已验证" : " · 邮箱未验证"}
          </p>
        </section>
        <QuotaCard quota={overview.quota} />
        <HistoryList jobs={overview.jobs} />
      </div>
    </div>
  );
}

export default UserCenterPage;
