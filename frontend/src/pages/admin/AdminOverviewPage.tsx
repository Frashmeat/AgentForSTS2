import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, ClipboardList, HeartPulse, KeyRound, RefreshCcw } from "lucide-react";

import {
  listAdminAuditEvents,
  listAdminExecutionProfiles,
  listAdminQuotaRefunds,
  listAdminServerCredentials,
  type AdminAuditEvent,
  type AdminExecutionProfileListItem,
  type AdminQuotaRefundItem,
  type AdminServerCredentialListItem,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { useSession } from "../../shared/session/hooks.ts";

type OverviewState = {
  profiles: AdminExecutionProfileListItem[];
  credentials: AdminServerCredentialListItem[];
  auditEvents: AdminAuditEvent[];
  refunds: AdminQuotaRefundItem[];
};

const emptyOverview: OverviewState = {
  profiles: [],
  credentials: [],
  auditEvents: [],
  refunds: [],
};

function countCredentials(credentials: AdminServerCredentialListItem[], status: string): number {
  return credentials.filter((credential) => credential.health_status === status).length;
}

function metricClass(tone: "violet" | "emerald" | "amber" | "rose") {
  const styles = {
    violet: "border-violet-100 bg-violet-50 text-violet-700",
    emerald: "border-emerald-100 bg-emerald-50 text-emerald-700",
    amber: "border-amber-100 bg-amber-50 text-amber-700",
    rose: "border-rose-100 bg-rose-50 text-rose-700",
  };
  return `rounded-lg border px-4 py-3 ${styles[tone]}`;
}

export function AdminOverviewPage() {
  const { isAuthenticated, isLoading, refreshSession } = useSession();
  const [overview, setOverview] = useState<OverviewState>(emptyOverview);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadOverview() {
    if (!isAuthenticated) {
      return;
    }
    setLoading(true);
    setError("");
    try {
      const [profileView, credentialView, auditEvents, refunds] = await Promise.all([
        listAdminExecutionProfiles(),
        listAdminServerCredentials(),
        listAdminAuditEvents(undefined, undefined, undefined, 8),
        listAdminQuotaRefunds(),
      ]);
      setOverview({
        profiles: profileView.items,
        credentials: credentialView.items,
        auditEvents,
        refunds,
      });
    } catch (loadError) {
      const message = resolveErrorMessage(loadError);
      if (message.includes("authentication required")) {
        void refreshSession();
      }
      setError(message || "读取管理台概览失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadOverview();
  }, [isAuthenticated]);

  const enabledProfiles = useMemo(
    () => overview.profiles.filter((profile) => profile.enabled).length,
    [overview.profiles],
  );
  const healthyCredentials = countCredentials(overview.credentials, "healthy");
  const riskyCredentials = overview.credentials.filter(
    (credential) => credential.health_status !== "healthy" && credential.health_status !== "disabled",
  ).length;
  const disabledCredentials = countCredentials(overview.credentials, "disabled");

  if (isLoading) {
    return <section className="rounded-lg border border-white bg-white/80 p-6 text-sm text-slate-500">正在恢复管理员会话...</section>;
  }

  if (!isAuthenticated) {
    return (
      <section className="rounded-lg border border-white bg-white/80 p-6">
        <h1 className="text-xl font-semibold text-slate-950">管理台</h1>
        <p className="mt-2 text-sm text-slate-500">登录后查看管理端数据。</p>
      </section>
    );
  }

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">管理台首页</h1>
          <p className="mt-1 text-sm text-slate-500">运行、服务器能力、额度返还与审计入口。</p>
        </div>
        <button
          type="button"
          onClick={() => {
            void loadOverview();
          }}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
          disabled={loading}
        >
          <RefreshCcw size={16} />
          <span>{loading ? "刷新中" : "刷新"}</span>
        </button>
      </header>

      {error ? (
        <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">{error}</section>
      ) : null}

      <section className="grid gap-3 md:grid-cols-4">
        <div className={metricClass("violet")}>
          <p className="text-xs font-semibold text-slate-500">可用执行配置</p>
          <p className="mt-2 text-2xl font-semibold">{enabledProfiles}</p>
        </div>
        <div className={metricClass("emerald")}>
          <p className="text-xs font-semibold text-slate-500">健康凭据</p>
          <p className="mt-2 text-2xl font-semibold">{healthyCredentials}</p>
        </div>
        <div className={metricClass("amber")}>
          <p className="text-xs font-semibold text-slate-500">需复检凭据</p>
          <p className="mt-2 text-2xl font-semibold">{riskyCredentials}</p>
        </div>
        <div className={metricClass("rose")}>
          <p className="text-xs font-semibold text-slate-500">已停用凭据</p>
          <p className="mt-2 text-2xl font-semibold">{disabledCredentials}</p>
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-[1.2fr_0.8fr]">
        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <div className="mb-3 flex items-center gap-2">
            <ClipboardList size={18} className="text-violet-700" />
            <h2 className="text-base font-semibold">服务器能力矩阵</h2>
          </div>
          <div className="overflow-x-auto">
            <table className="min-w-full text-left text-sm">
              <thead className="text-xs text-slate-500">
                <tr>
                  <th className="px-3 py-2 font-semibold">执行配置</th>
                  <th className="px-3 py-2 font-semibold">模型</th>
                  <th className="px-3 py-2 font-semibold">状态</th>
                  <th className="px-3 py-2 font-semibold">用户可选</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {overview.profiles.slice(0, 6).map((profile) => (
                  <tr key={profile.id}>
                    <td className="px-3 py-2 font-medium text-slate-800">{profile.display_name}</td>
                    <td className="px-3 py-2 text-slate-600">{profile.model}</td>
                    <td className="px-3 py-2 text-slate-600">{profile.enabled ? "可用" : "已停用"}</td>
                    <td className="px-3 py-2 text-slate-600">{profile.recommended ? "推荐" : "普通"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div className="space-y-4">
          <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
            <div className="mb-3 flex items-center gap-2">
              <KeyRound size={18} className="text-violet-700" />
              <h2 className="text-base font-semibold">待处理事项</h2>
            </div>
            <div className="space-y-2 text-sm text-slate-600">
              <p className="flex items-center justify-between gap-3">
                <span>需复检凭据</span>
                <strong className="text-slate-900">{riskyCredentials}</strong>
              </p>
              <p className="flex items-center justify-between gap-3">
                <span>停用凭据</span>
                <strong className="text-slate-900">{disabledCredentials}</strong>
              </p>
              <p className="flex items-center justify-between gap-3">
                <span>最近审计事件</span>
                <strong className="text-slate-900">{overview.auditEvents.length}</strong>
              </p>
            </div>
          </section>

          <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
            <div className="mb-3 flex items-center gap-2">
              <HeartPulse size={18} className="text-violet-700" />
              <h2 className="text-base font-semibold">最近退款</h2>
            </div>
            <div className="space-y-2 text-sm text-slate-600">
              {overview.refunds.slice(0, 4).map((refund, index) => (
                <p key={`${refund.ai_execution_id}-${index}`} className="flex items-center justify-between gap-3">
                  <span>执行编号 {refund.ai_execution_id}</span>
                  <strong className="text-slate-900">{refund.charge_status}</strong>
                </p>
              ))}
              {overview.refunds.length === 0 ? (
                <p className="flex items-center gap-2 text-slate-500">
                  <AlertTriangle size={15} />
                  <span>暂无退款记录</span>
                </p>
              ) : null}
            </div>
          </section>
        </div>
      </section>
    </div>
  );
}

export default AdminOverviewPage;
