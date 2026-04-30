import { useEffect, useMemo, useState } from "react";
import { Plus, RefreshCcw, Save, ServerCog, Trash2 } from "lucide-react";

import {
  createAdminExecutionProfile,
  deleteAdminExecutionProfile,
  disableAdminExecutionProfile,
  enableAdminExecutionProfile,
  listAdminExecutionProfiles,
  listAdminServerCredentials,
  updateAdminExecutionProfile,
  type AdminExecutionProfileListItem,
  type AdminServerCredentialListItem,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminProvider, formatAdminStatus } from "./adminDisplay.ts";

type ExecutionProfileFormState = {
  id: number | null;
  code: string;
  display_name: string;
  agent_backend: "codex" | "claude";
  model: string;
  description: string;
  enabled: boolean;
  recommended: boolean;
  sort_order: number;
};

const emptyForm: ExecutionProfileFormState = {
  id: null,
  code: "",
  display_name: "",
  agent_backend: "codex",
  model: "",
  description: "",
  enabled: true,
  recommended: false,
  sort_order: 0,
};

function toForm(profile: AdminExecutionProfileListItem): ExecutionProfileFormState {
  return {
    id: profile.id,
    code: profile.code,
    display_name: profile.display_name,
    agent_backend: profile.agent_backend === "claude" ? "claude" : "codex",
    model: profile.model,
    description: profile.description ?? "",
    enabled: profile.enabled,
    recommended: profile.recommended,
    sort_order: profile.sort_order,
  };
}

export function AdminExecutionProfilesPage() {
  const [profiles, setProfiles] = useState<AdminExecutionProfileListItem[]>([]);
  const [credentials, setCredentials] = useState<AdminServerCredentialListItem[]>([]);
  const [form, setForm] = useState<ExecutionProfileFormState>(emptyForm);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
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

  function patchForm(patch: Partial<ExecutionProfileFormState>) {
    setForm((current) => ({ ...current, ...patch }));
  }

  function startCreate() {
    setMessage("");
    setError("");
    setForm(emptyForm);
  }

  function startEdit(profile: AdminExecutionProfileListItem) {
    setMessage("");
    setError("");
    setForm(toForm(profile));
  }

  async function submitForm() {
    setSaving(true);
    setError("");
    setMessage("");
    try {
      const payload = {
        code: form.code,
        display_name: form.display_name,
        agent_backend: form.agent_backend,
        model: form.model,
        description: form.description,
        enabled: form.enabled,
        recommended: form.recommended,
        sort_order: Number(form.sort_order) || 0,
      };
      if (form.id === null) {
        await createAdminExecutionProfile(payload);
        setMessage("执行配置已新增。");
      } else {
        await updateAdminExecutionProfile(form.id, payload);
        setMessage("执行配置已保存。");
      }
      await loadData();
      setForm(emptyForm);
    } catch (submitError) {
      setError(resolveErrorMessage(submitError) || "执行配置保存失败");
    } finally {
      setSaving(false);
    }
  }

  async function runProfileAction(action: () => Promise<unknown>, successMessage: string) {
    setSaving(true);
    setError("");
    setMessage("");
    try {
      await action();
      setMessage(successMessage);
      await loadData();
    } catch (actionError) {
      setError(resolveErrorMessage(actionError) || "执行配置操作失败");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mx-auto max-w-7xl space-y-5">
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

      {error ? (
        <section className="rounded-lg border border-rose-100 bg-rose-50 px-4 py-3 text-sm text-rose-700">
          {error}
        </section>
      ) : null}
      {message ? (
        <section className="rounded-lg border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
          {message}
        </section>
      ) : null}

      <section className="grid items-start gap-4 xl:grid-cols-[420px_minmax(0,1fr)]">
        <div className="rounded-lg border border-white bg-white/85 p-4 shadow-sm">
          <div className="mb-4 flex items-center justify-between gap-3">
            <div>
              <h2 className="text-base font-semibold text-slate-900">配置表单</h2>
              <p className="mt-1 text-xs text-slate-500">
                {form.id === null ? "新增执行配置" : `正在编辑 #${form.id}`}
              </p>
            </div>
            <button
              type="button"
              onClick={startCreate}
              className="inline-flex items-center gap-1 rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
            >
              <Plus size={14} />
              <span>新增</span>
            </button>
          </div>
          <div className="grid gap-3">
            <label className="space-y-1 text-sm text-slate-600">
              <span>编号</span>
              <input
                value={form.code}
                onChange={(event) => patchForm({ code: event.target.value })}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
                placeholder="codex-gpt-5-5"
              />
            </label>
            <label className="space-y-1 text-sm text-slate-600">
              <span>名称</span>
              <input
                value={form.display_name}
                onChange={(event) => patchForm({ display_name: event.target.value })}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
                placeholder="Codex CLI / gpt-5.5"
              />
            </label>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-1 2xl:grid-cols-2">
              <label className="space-y-1 text-sm text-slate-600">
                <span>Agent 后端</span>
                <select
                  value={form.agent_backend}
                  onChange={(event) =>
                    patchForm({ agent_backend: event.target.value === "claude" ? "claude" : "codex" })
                  }
                  className="w-full rounded-lg border border-slate-200 px-3 py-2"
                >
                  <option value="codex">Codex</option>
                  <option value="claude">Claude</option>
                </select>
              </label>
              <label className="space-y-1 text-sm text-slate-600">
                <span>模型</span>
                <input
                  value={form.model}
                  onChange={(event) => patchForm({ model: event.target.value })}
                  className="w-full rounded-lg border border-slate-200 px-3 py-2"
                  placeholder="gpt-5.5"
                />
              </label>
            </div>
            <label className="space-y-1 text-sm text-slate-600">
              <span>描述</span>
              <textarea
                value={form.description}
                onChange={(event) => patchForm({ description: event.target.value })}
                className="min-h-20 w-full rounded-lg border border-slate-200 px-3 py-2"
              />
            </label>
            <label className="space-y-1 text-sm text-slate-600">
              <span>排序</span>
              <input
                type="number"
                value={form.sort_order}
                onChange={(event) => patchForm({ sort_order: Number(event.target.value) || 0 })}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
              />
            </label>
          </div>
          <div className="mt-3 flex flex-wrap gap-4">
            <label className="inline-flex items-center gap-2 text-sm text-slate-600">
              <input
                type="checkbox"
                checked={form.enabled}
                onChange={(event) => patchForm({ enabled: event.target.checked })}
              />
              <span>启用配置</span>
            </label>
            <label className="inline-flex items-center gap-2 text-sm text-slate-600">
              <input
                type="checkbox"
                checked={form.recommended}
                onChange={(event) => patchForm({ recommended: event.target.checked })}
              />
              <span>推荐配置</span>
            </label>
          </div>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void submitForm()}
              disabled={saving || !form.code || !form.display_name || !form.model}
              className="inline-flex items-center gap-2 rounded-lg bg-violet-700 px-3 py-2 text-sm font-medium text-white transition hover:bg-violet-800 disabled:opacity-50"
            >
              <Save size={16} />
              <span>{form.id === null ? "新增执行配置" : "保存执行配置"}</span>
            </button>
            <button
              type="button"
              onClick={startCreate}
              className="rounded-lg border border-slate-200 px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
            >
              清空表单
            </button>
          </div>
        </div>

        <div className="min-w-0 rounded-lg border border-white bg-white/85 p-4 shadow-sm">
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
                  <th className="px-3 py-2 font-semibold">操作</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {profiles.map((profile) => {
                  const profileCredentials = credentialsByProfile.get(profile.id) ?? [];
                  const enabledCredentials = profileCredentials.filter((credential) => credential.enabled);
                  const providers = [
                    ...new Set(profileCredentials.map((credential) => formatAdminProvider(credential.provider))),
                  ];
                  return (
                    <tr key={profile.id}>
                      <td className="px-3 py-2 font-medium text-slate-900">{profile.id}</td>
                      <td className="px-3 py-2 text-slate-700">
                        <div className="font-medium">{profile.display_name}</div>
                        <div className="text-xs text-slate-400">{profile.code}</div>
                      </td>
                      <td className="px-3 py-2 text-slate-600">
                        {providers.length ? providers.join(" / ") : formatAdminProvider(profile.agent_backend)}
                      </td>
                      <td className="px-3 py-2 text-slate-600">{profile.model}</td>
                      <td className="px-3 py-2 text-slate-600">
                        {enabledCredentials.length} / {profileCredentials.length}
                      </td>
                      <td className="px-3 py-2 text-slate-600">
                        {profile.enabled
                          ? profile.recommended
                            ? "推荐可选"
                            : "可选"
                          : formatAdminStatus("disabled").label}
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex flex-wrap gap-2">
                          <button
                            type="button"
                            onClick={() => startEdit(profile)}
                            className="rounded-lg border border-slate-200 px-2.5 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
                          >
                            编辑
                          </button>
                          {profile.enabled ? (
                            <button
                              type="button"
                              onClick={() =>
                                void runProfileAction(
                                  () => disableAdminExecutionProfile(profile.id),
                                  "执行配置已禁用。",
                                )
                              }
                              className="rounded-lg border border-slate-200 px-2.5 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
                              disabled={saving}
                            >
                              禁用
                            </button>
                          ) : (
                            <button
                              type="button"
                              onClick={() =>
                                void runProfileAction(() => enableAdminExecutionProfile(profile.id), "执行配置已启用。")
                              }
                              className="rounded-lg border border-slate-200 px-2.5 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
                              disabled={saving}
                            >
                              启用
                            </button>
                          )}
                          <button
                            type="button"
                            onClick={() =>
                              void runProfileAction(() => deleteAdminExecutionProfile(profile.id), "执行配置已删除。")
                            }
                            className="inline-flex items-center gap-1 rounded-lg border border-rose-100 px-2.5 py-1.5 text-xs text-rose-600 transition hover:border-rose-200 hover:bg-rose-50"
                            disabled={saving}
                          >
                            <Trash2 size={13} />
                            <span>删除</span>
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      </section>
    </div>
  );
}

export default AdminExecutionProfilesPage;
