import { useEffect, useMemo, useState } from "react";
import { RefreshCcw, ServerCog } from "lucide-react";

import {
  listAdminExecutionProfiles,
  listAdminServerCredentials,
  type AdminExecutionProfileListItem,
  type AdminServerCredentialListItem,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminProvider, formatAdminStatus } from "./adminDisplay.ts";

export function AdminExecutionProfilesPage() {
  const [profiles, setProfiles] = useState<AdminExecutionProfileListItem[]>([]);
  const [credentials, setCredentials] = useState<AdminServerCredentialListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

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
      setError(resolveErrorMessage(loadError) || "读取执行配置失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadData();
  }, []);

  const credentialsByProfile = useMemo(() => {
    const grouped = new Map<number, AdminServerCredentialListItem[]>();
    credentials.forEach((credential) => {
      const items = grouped.get(credential.execution_profile_id) ?? [];
      items.push(credential);
      grouped.set(credential.execution_profile_id, items);
    });
    return grouped;
  }, [credentials]);

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">执行配置</h1>
          <p className="mt-1 text-sm text-slate-500">模型组合、服务商和可用凭据。</p>
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
          <span>{loading ? "刷新中" : "刷新配置"}</span>
        </button>
      </header>

      {error ? <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">{error}</section> : null}

      <section className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
        <div className="mb-3 flex items-center gap-2">
          <ServerCog size={18} className="text-violet-700" />
          <h2 className="text-base font-semibold text-slate-900">执行配置列表</h2>
        </div>
        <div className="overflow-x-auto">
          <table className="min-w-full text-left text-sm">
            <thead className="text-xs text-slate-500">
              <tr>
                <th className="px-3 py-2 font-semibold">编号</th>
                <th className="px-3 py-2 font-semibold">名称</th>
                <th className="px-3 py-2 font-semibold">服务商</th>
                <th className="px-3 py-2 font-semibold">模型</th>
                <th className="px-3 py-2 font-semibold">可用凭据</th>
                <th className="px-3 py-2 font-semibold">用户可选状态</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {profiles.map((profile) => {
                const profileCredentials = credentialsByProfile.get(profile.id) ?? [];
                const enabledCredentials = profileCredentials.filter((credential) => credential.enabled);
                const providers = [...new Set(profileCredentials.map((credential) => formatAdminProvider(credential.provider)))];
                return (
                  <tr key={profile.id}>
                    <td className="px-3 py-2 font-medium text-slate-900">{profile.id}</td>
                    <td className="px-3 py-2 text-slate-700">{profile.display_name}</td>
                    <td className="px-3 py-2 text-slate-600">{providers.length ? providers.join(" / ") : formatAdminProvider(profile.agent_backend)}</td>
                    <td className="px-3 py-2 text-slate-600">{profile.model}</td>
                    <td className="px-3 py-2 text-slate-600">{enabledCredentials.length} / {profileCredentials.length}</td>
                    <td className="px-3 py-2 text-slate-600">{profile.enabled ? (profile.recommended ? "推荐可选" : "可选") : formatAdminStatus("disabled").label}</td>
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

export default AdminExecutionProfilesPage;
