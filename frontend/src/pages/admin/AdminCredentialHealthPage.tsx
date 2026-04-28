import { useEffect, useMemo, useState } from "react";
import { HeartPulse, RefreshCcw } from "lucide-react";

import {
  listAdminExecutionProfiles,
  listAdminServerCredentials,
  runAdminServerCredentialHealthCheck,
  type AdminExecutionProfileListItem,
  type AdminServerCredentialListItem,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminProvider, formatAdminStatus } from "./adminDisplay.ts";

function formatTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未检测";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

function statusClass(status: string): string {
  const tone = formatAdminStatus(status).tone;
  switch (tone) {
    case "success":
      return "border-emerald-100 bg-emerald-50 text-emerald-700";
    case "warning":
      return "border-amber-100 bg-amber-50 text-amber-700";
    case "danger":
      return "border-rose-100 bg-rose-50 text-rose-700";
    default:
      return "border-slate-100 bg-slate-50 text-slate-600";
  }
}

export function AdminCredentialHealthPage() {
  const [profiles, setProfiles] = useState<AdminExecutionProfileListItem[]>([]);
  const [credentials, setCredentials] = useState<AdminServerCredentialListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  const profileById = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile])), [profiles]);

  async function loadData() {
    setLoading(true);
    setError("");
    try {
      const [profileView, credentialView] = await Promise.all([
        listAdminExecutionProfiles(),
        listAdminServerCredentials(),
      ]);
      setProfiles(profileView.items);
      setCredentials(credentialView.items);
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取健康检查数据失败");
    } finally {
      setLoading(false);
    }
  }

  async function runHealthCheck(credentialId: number) {
    setSaving(true);
    setMessage("");
    setError("");
    try {
      await runAdminServerCredentialHealthCheck(credentialId);
      setMessage("健康检查已完成。");
      await loadData();
    } catch (checkError) {
      setError(resolveErrorMessage(checkError) || "健康检查失败");
    } finally {
      setSaving(false);
    }
  }

  useEffect(() => {
    void loadData();
  }, []);

  const summary = {
    healthy: credentials.filter((credential) => credential.health_status === "healthy").length,
    degraded: credentials.filter((credential) => credential.health_status === "degraded").length,
    authFailed: credentials.filter((credential) => credential.health_status === "auth_failed").length,
    rateLimited: credentials.filter((credential) => credential.health_status === "rate_limited").length,
    quotaExhausted: credentials.filter((credential) => credential.health_status === "quota_exhausted").length,
    disabled: credentials.filter((credential) => credential.health_status === "disabled").length,
  };

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">健康检查</h1>
          <p className="mt-1 text-sm text-slate-500">服务器凭据健康状态与最近检测结果。</p>
        </div>
        <button
          type="button"
          onClick={() => {
            void loadData();
          }}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
          disabled={loading}
        >
          <RefreshCcw size={16} />
          <span>{loading ? "刷新中" : "刷新"}</span>
        </button>
      </header>

      {error ? <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">{error}</section> : null}
      {message ? <section className="rounded-lg border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">{message}</section> : null}

      <section className="grid gap-3 md:grid-cols-6">
        {[
          ["健康", summary.healthy, "healthy"],
          ["需复检", summary.degraded, "degraded"],
          ["认证失败", summary.authFailed, "auth_failed"],
          ["调用限流", summary.rateLimited, "rate_limited"],
          ["额度耗尽", summary.quotaExhausted, "quota_exhausted"],
          ["已停用", summary.disabled, "disabled"],
        ].map(([label, value, status]) => (
          <div key={String(status)} className={`rounded-lg border px-4 py-3 ${statusClass(String(status))}`}>
            <p className="text-xs font-semibold text-slate-500">{label}</p>
            <p className="mt-2 text-2xl font-semibold">{value}</p>
          </div>
        ))}
      </section>

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <div className="mb-3 flex items-center gap-2">
          <HeartPulse size={18} className="text-violet-700" />
          <h2 className="text-base font-semibold text-slate-900">检查结果</h2>
        </div>
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="text-xs text-slate-500">
              <tr>
                <th className="px-3 py-2 font-semibold">凭据</th>
                <th className="px-3 py-2 font-semibold">执行配置</th>
                <th className="px-3 py-2 font-semibold">服务商</th>
                <th className="px-3 py-2 font-semibold">状态</th>
                <th className="px-3 py-2 font-semibold">最近检查</th>
                <th className="px-3 py-2 font-semibold">问题</th>
                <th className="px-3 py-2 font-semibold">操作</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {credentials.map((credential) => {
                const status = formatAdminStatus(credential.health_status);
                return (
                  <tr key={credential.id}>
                    <td className="px-3 py-2 font-medium text-slate-900">{credential.label || `凭据 ${credential.id}`}</td>
                    <td className="px-3 py-2 text-slate-600">{profileById.get(credential.execution_profile_id)?.display_name ?? "未知配置"}</td>
                    <td className="px-3 py-2 text-slate-600">{formatAdminProvider(credential.provider)}</td>
                    <td className="px-3 py-2"><span className={`rounded-md border px-2 py-1 text-xs font-medium ${statusClass(credential.health_status)}`}>{status.label}</span></td>
                    <td className="px-3 py-2 text-slate-600">{formatTime(credential.last_checked_at)}</td>
                    <td className="px-3 py-2 text-slate-600">{credential.last_error_message || "无"}</td>
                    <td className="px-3 py-2">
                      <button
                        type="button"
                        onClick={() => void runHealthCheck(credential.id)}
                        className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700 disabled:opacity-50"
                        disabled={saving || !credential.enabled}
                      >
                        健康检查
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

export default AdminCredentialHealthPage;
