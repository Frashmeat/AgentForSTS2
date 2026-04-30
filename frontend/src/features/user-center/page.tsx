import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { House } from "lucide-react";
import { PlatformPageShell } from "../../components/platform/PlatformPageShell.tsx";
import { HistoryList } from "./historyList.tsx";
import { loadUserCenterOverview, type UserCenterOverview } from "./model.ts";
import { QuotaCard } from "./quotaCard.tsx";
import { resolveErrorMessage } from "../../shared/error.ts";
import { useSession } from "../../shared/session/hooks.ts";

function toFriendlyUserCenterErrorMessage(message: string): string {
  if (message.includes("authentication required")) {
    return "当前登录状态已失效。若当前是 hybrid / 跨域部署，请检查 Web 后端 Cookie 的 SameSite、Secure 和 HTTPS 配置，然后重新登录。";
  }
  return message;
}

export function UserCenterPage() {
  const { isAuthenticated, isLoading, refreshSession } = useSession();
  const [overview, setOverview] = useState<UserCenterOverview | null>(null);
  const [error, setError] = useState("");
  const homeAction = (
    <Link to="/" className="platform-page-action-link">
      <House size={16} />
      <span>返回首页</span>
    </Link>
  );

  useEffect(() => {
    if (!isAuthenticated) {
      return;
    }
    let cancelled = false;
    void loadUserCenterOverview()
      .then((result) => {
        if (!cancelled) {
          setOverview(result);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          const message = toFriendlyUserCenterErrorMessage(resolveErrorMessage(err) || "加载用户中心失败");
          if (message.includes("当前登录状态已失效")) {
            void refreshSession();
          }
          setError(message);
        }
      });

    return () => {
      cancelled = true;
    };
    // refreshSession 由 session store 提供且每次 render 引用变化；仅在 isAuthenticated 变化时拉取。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isAuthenticated]);

  if (isLoading) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="用户中心"
        description="正在恢复会话并同步平台账号状态。"
        actions={homeAction}
      >
        <section className="platform-page-card p-8 text-sm text-slate-500">正在恢复会话...</section>
      </PlatformPageShell>
    );
  }

  if (!isAuthenticated) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="用户中心"
        description="登录后查看平台任务、统一次数池和返还记录。"
        actions={homeAction}
      >
        <section className="platform-page-card p-8">
          <h1 className="text-2xl font-semibold text-slate-900">登录后查看用户中心</h1>
          <p className="mt-2 text-sm text-slate-500">平台任务、统一次数池和返还记录都绑定当前账号。</p>
          {error ? <p className="mt-4 text-sm text-rose-600">{error}</p> : null}
          <Link to="/auth/login" className="platform-page-primary-button mt-6 inline-flex">
            去登录
          </Link>
        </section>
      </PlatformPageShell>
    );
  }

  if (error) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="用户中心"
        description="平台数据加载失败，请检查登录态或后端配置。"
        actions={homeAction}
      >
        <section className="platform-page-card p-8 text-sm text-rose-600">{error}</section>
      </PlatformPageShell>
    );
  }

  if (overview === null) {
    return (
      <PlatformPageShell
        kicker="User Center"
        title="用户中心"
        description="正在读取平台任务与配额信息。"
        actions={homeAction}
      >
        <section className="platform-page-card p-8 text-sm text-slate-500">加载用户中心中...</section>
      </PlatformPageShell>
    );
  }

  return (
    <PlatformPageShell
      kicker="User Center"
      title={overview.profile.username}
      description={
        <>
          <p>
            {overview.profile.email}
            {overview.profile.email_verified ? " · 邮箱已验证" : " · 邮箱未验证"}
          </p>
          <p className="mt-2">这里只读取平台模式任务，不混入本机 BYOK、localStorage 恢复快照或 Agent 本地日志。</p>
        </>
      }
      actions={homeAction}
    >
      <QuotaCard quota={overview.quota} />
      <HistoryList jobs={overview.jobs} />
    </PlatformPageShell>
  );
}

export default UserCenterPage;
