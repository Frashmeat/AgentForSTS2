import { useEffect, useMemo, useState } from "react";
import { Plus, RefreshCcw, Save } from "lucide-react";

import {
  createAdminServerCredential,
  disableAdminServerCredential,
  enableAdminServerCredential,
  listAdminExecutionProfiles,
  listAdminServerCredentials,
  runAdminServerCredentialHealthCheck,
  updateAdminServerCredential,
  type AdminExecutionProfileListItem,
  type AdminServerCredentialListItem,
} from "../../shared/api/index.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { formatAdminAuthType, formatAdminProvider, formatAdminStatus } from "./adminDisplay.ts";

type CredentialFormState = {
  id: number | null;
  execution_profile_id: number;
  provider: "openai" | "anthropic";
  auth_type: "api_key" | "ak_sk";
  credential: string;
  secret: string;
  base_url: string;
  label: string;
  priority: number;
  enabled: boolean;
};

const emptyForm: CredentialFormState = {
  id: null,
  execution_profile_id: 0,
  provider: "openai",
  auth_type: "api_key",
  credential: "",
  secret: "",
  base_url: "",
  label: "",
  priority: 0,
  enabled: true,
};

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

export function AdminServerCredentialsPage() {
  const [profiles, setProfiles] = useState<AdminExecutionProfileListItem[]>([]);
  const [credentials, setCredentials] = useState<AdminServerCredentialListItem[]>([]);
  const [selectedProfileId, setSelectedProfileId] = useState<number | null>(null);
  const [form, setForm] = useState<CredentialFormState>(emptyForm);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  const profileById = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile])), [profiles]);

  function patchForm(patch: Partial<CredentialFormState>) {
    setForm((current) => ({ ...current, ...patch }));
  }

  async function loadData(profileId = selectedProfileId) {
    setLoading(true);
    setError("");
    try {
      const [profileView, credentialView] = await Promise.all([
        listAdminExecutionProfiles(),
        listAdminServerCredentials(profileId ?? undefined),
      ]);
      setProfiles(profileView.items);
      setCredentials(credentialView.items);
      if (!form.execution_profile_id && profileView.items[0]) {
        patchForm({ execution_profile_id: profileView.items[0].id });
      }
    } catch (loadError) {
      setError(resolveErrorMessage(loadError) || "读取服务器凭据失败");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadData();
    // mount-only：进入页面时拉一次数据，loadData 是组件内闭包不进 deps。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function startCreate() {
    setMessage("");
    setError("");
    setForm({
      ...emptyForm,
      execution_profile_id: selectedProfileId ?? profiles[0]?.id ?? 0,
    });
  }

  function startEdit(credential: AdminServerCredentialListItem) {
    setMessage("");
    setError("");
    setForm({
      id: credential.id,
      execution_profile_id: credential.execution_profile_id,
      provider: credential.provider === "anthropic" ? "anthropic" : "openai",
      auth_type: credential.auth_type === "ak_sk" ? "ak_sk" : "api_key",
      credential: "",
      secret: "",
      base_url: credential.base_url,
      label: credential.label,
      priority: credential.priority,
      enabled: credential.enabled,
    });
  }

  async function submitForm() {
    setSaving(true);
    setError("");
    setMessage("");
    try {
      const payload = {
        execution_profile_id: Number(form.execution_profile_id),
        provider: form.provider,
        auth_type: form.auth_type,
        credential: form.credential,
        secret: form.secret,
        base_url: form.base_url,
        label: form.label,
        priority: Number(form.priority) || 0,
        enabled: form.enabled,
      };
      if (form.id === null) {
        await createAdminServerCredential(payload);
        setMessage("服务器凭据已新增。");
      } else {
        await updateAdminServerCredential(form.id, payload);
        setMessage("服务器凭据已保存。");
      }
      await loadData();
      setForm({
        ...emptyForm,
        execution_profile_id: selectedProfileId ?? profiles[0]?.id ?? 0,
      });
    } catch (submitError) {
      setError(resolveErrorMessage(submitError) || "服务器凭据保存失败");
    } finally {
      setSaving(false);
    }
  }

  async function runCredentialAction(action: () => Promise<unknown>, successMessage: string) {
    setSaving(true);
    setError("");
    setMessage("");
    try {
      await action();
      setMessage(successMessage);
      await loadData();
    } catch (actionError) {
      setError(resolveErrorMessage(actionError) || "服务器凭据操作失败");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mx-auto max-w-7xl space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-slate-950">服务器凭据</h1>
          <p className="mt-1 text-sm text-slate-500">新增、编辑、启用、禁用和检测服务器凭据。</p>
        </div>
        <button
          type="button"
          onClick={() => void loadData()}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
          disabled={loading}
        >
          <RefreshCcw size={16} />
          <span>{loading ? "刷新中" : "刷新凭据"}</span>
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
              <h2 className="text-base font-semibold text-slate-900">凭据表单</h2>
              <p className="mt-1 text-xs text-slate-500">
                {form.id === null ? "新增服务器模型凭据" : `正在编辑 #${form.id}`}
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
              <span>执行配置</span>
              <select
                value={form.execution_profile_id || ""}
                onChange={(event) => patchForm({ execution_profile_id: Number(event.target.value) })}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
              >
                <option value="">请选择</option>
                {profiles.map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.display_name}
                  </option>
                ))}
              </select>
            </label>
            <label className="space-y-1 text-sm text-slate-600">
              <span>名称</span>
              <input
                value={form.label}
                onChange={(event) => patchForm({ label: event.target.value })}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
              />
            </label>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-1 2xl:grid-cols-2">
              <label className="space-y-1 text-sm text-slate-600">
                <span>服务商</span>
                <select
                  value={form.provider}
                  onChange={(event) =>
                    patchForm({ provider: event.target.value === "anthropic" ? "anthropic" : "openai" })
                  }
                  className="w-full rounded-lg border border-slate-200 px-3 py-2"
                >
                  <option value="openai">{formatAdminProvider("openai")}</option>
                  <option value="anthropic">{formatAdminProvider("anthropic")}</option>
                </select>
              </label>
              <label className="space-y-1 text-sm text-slate-600">
                <span>认证方式</span>
                <select
                  value={form.auth_type}
                  onChange={(event) => patchForm({ auth_type: event.target.value === "ak_sk" ? "ak_sk" : "api_key" })}
                  className="w-full rounded-lg border border-slate-200 px-3 py-2"
                >
                  <option value="api_key">{formatAdminAuthType("api_key")}</option>
                  <option value="ak_sk">{formatAdminAuthType("ak_sk")}</option>
                </select>
              </label>
            </div>
            <label className="space-y-1 text-sm text-slate-600">
              <span>密钥</span>
              <input
                value={form.credential}
                onChange={(event) => patchForm({ credential: event.target.value })}
                placeholder={form.id ? "留空表示保留原值" : "输入 API Key / AK"}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
              />
            </label>
            <label className="space-y-1 text-sm text-slate-600">
              <span>Secret</span>
              <input
                value={form.secret}
                onChange={(event) => patchForm({ secret: event.target.value })}
                placeholder="留空表示保留原值"
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
              />
            </label>
            <label className="space-y-1 text-sm text-slate-600">
              <span>Base URL</span>
              <input
                value={form.base_url}
                onChange={(event) => patchForm({ base_url: event.target.value })}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
              />
            </label>
            <label className="space-y-1 text-sm text-slate-600">
              <span>优先级</span>
              <input
                type="number"
                value={form.priority}
                onChange={(event) => patchForm({ priority: Number(event.target.value) || 0 })}
                className="w-full rounded-lg border border-slate-200 px-3 py-2"
              />
            </label>
          </div>
          <label className="mt-3 inline-flex items-center gap-2 text-sm text-slate-600">
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={(event) => patchForm({ enabled: event.target.checked })}
            />
            <span>启用凭据</span>
          </label>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void submitForm()}
              disabled={saving || !form.execution_profile_id}
              className="inline-flex items-center gap-2 rounded-lg bg-violet-700 px-3 py-2 text-sm font-medium text-white transition hover:bg-violet-800 disabled:opacity-50"
            >
              <Save size={16} />
              <span>{form.id === null ? "新增服务器凭据" : "保存服务器凭据"}</span>
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
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-base font-semibold text-slate-900">凭据列表</h2>
            <select
              value={selectedProfileId ?? ""}
              onChange={(event) => {
                const value = event.target.value ? Number(event.target.value) : null;
                setSelectedProfileId(value);
                void loadData(value);
              }}
              className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
            >
              <option value="">全部执行配置</option>
              {profiles.map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.display_name}
                </option>
              ))}
            </select>
          </div>
          {credentials.length === 0 ? (
            <p className="rounded-lg border border-dashed border-slate-200 px-4 py-6 text-sm text-slate-500">
              当前筛选条件下没有服务器凭据。
            </p>
          ) : (
            <div className="grid gap-3 2xl:grid-cols-2">
              {credentials.map((credential) => {
                const status = formatAdminStatus(credential.health_status);
                return (
                  <article key={credential.id} className="rounded-lg border border-slate-100 bg-slate-50 px-4 py-3">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <p className="text-sm font-semibold text-slate-900">
                          {credential.label || `凭据 ${credential.id}`}
                        </p>
                        <p className="mt-1 text-xs text-slate-500">
                          {profileById.get(credential.execution_profile_id)?.display_name ?? "未知配置"}
                        </p>
                        <p className="mt-1 text-xs text-slate-500">
                          {formatAdminProvider(credential.provider)} / {formatAdminAuthType(credential.auth_type)} /
                          优先级 {credential.priority}
                        </p>
                        <p className="mt-1 break-all text-xs text-slate-500">
                          {credential.base_url || "未配置 Base URL"}
                        </p>
                        {credential.last_error_message ? (
                          <p className="mt-1 text-xs text-rose-600">{credential.last_error_message}</p>
                        ) : null}
                      </div>
                      <div className="text-right text-xs">
                        <p
                          className={`inline-flex rounded-md border px-2 py-1 font-semibold ${statusClass(credential.health_status)}`}
                        >
                          {status.label}
                        </p>
                        <p className="mt-1 text-slate-500">{credential.enabled ? "已启用" : "已停用"}</p>
                        <p className="mt-1 text-slate-500">{formatTime(credential.last_checked_at)}</p>
                      </div>
                    </div>
                    <div className="mt-3 flex flex-wrap gap-2">
                      <button
                        type="button"
                        onClick={() => startEdit(credential)}
                        className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
                      >
                        编辑
                      </button>
                      {credential.enabled ? (
                        <button
                          type="button"
                          onClick={() =>
                            void runCredentialAction(
                              () => disableAdminServerCredential(credential.id),
                              "服务器凭据已禁用。",
                            )
                          }
                          className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
                          disabled={saving}
                        >
                          禁用
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={() =>
                            void runCredentialAction(
                              () => enableAdminServerCredential(credential.id),
                              "服务器凭据已启用。",
                            )
                          }
                          className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700"
                          disabled={saving}
                        >
                          启用
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() =>
                          void runCredentialAction(
                            () => runAdminServerCredentialHealthCheck(credential.id),
                            "服务器凭据健康检查已完成。",
                          )
                        }
                        className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-violet-200 hover:text-violet-700 disabled:opacity-50"
                        disabled={saving || !credential.enabled}
                      >
                        健康检查
                      </button>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

export default AdminServerCredentialsPage;
