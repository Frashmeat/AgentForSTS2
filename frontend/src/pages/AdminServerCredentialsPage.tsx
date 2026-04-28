import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, RefreshCcw, ShieldCheck } from "lucide-react";
import { Link } from "react-router-dom";

import { PlatformPageShell } from "../components/platform/PlatformPageShell.tsx";
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
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";
import { useSession } from "../shared/session/hooks.ts";

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

function formatCredentialTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "未检测";
  }
  const date = new Date(text);
  return Number.isNaN(date.getTime()) ? text : date.toLocaleString("zh-CN", { hour12: false });
}

function healthTone(status: string): string {
  switch (status) {
    case "healthy":
      return "text-emerald-700";
    case "disabled":
      return "text-slate-500";
    case "auth_failed":
    case "quota_exhausted":
      return "text-rose-700";
    default:
      return "text-amber-700";
  }
}

function profileLabel(profile: AdminExecutionProfileListItem | undefined): string {
  if (!profile) {
    return "未知配置";
  }
  return `${profile.display_name} (${profile.code})`;
}

export function AdminServerCredentialsPage() {
  const { isAuthenticated, isLoading, refreshSession } = useSession();
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

  function toFriendlyError(errorValue: unknown): string {
    const resolved = resolveErrorMessage(errorValue);
    if (resolved.includes("admin permission required")) {
      return "当前账号没有管理员权限，无法管理服务器凭据。";
    }
    if (resolved.includes("authentication required")) {
      void refreshSession();
      return "当前登录状态已失效，请重新登录后再管理服务器凭据。";
    }
    return resolved || "服务器凭据操作失败";
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
      setError(toFriendlyError(loadError));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (!isAuthenticated) {
      return;
    }
    void loadData();
  }, [isAuthenticated]);

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
      startCreate();
    } catch (submitError) {
      setError(toFriendlyError(submitError));
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
      setError(toFriendlyError(actionError));
    } finally {
      setSaving(false);
    }
  }

  const backAction = (
    <Link to="/admin/runtime-audit" className="platform-page-action-link">
      <ArrowLeft size={16} />
      <span>返回审计</span>
    </Link>
  );

  if (isLoading) {
    return (
      <PlatformPageShell kicker="Admin Server Credentials" title="服务器凭据" description="正在恢复管理员会话状态。" actions={backAction}>
        <section className="platform-page-card p-8 text-sm text-slate-500">正在恢复会话...</section>
      </PlatformPageShell>
    );
  }

  if (!isAuthenticated) {
    return (
      <PlatformPageShell kicker="Admin Server Credentials" title="服务器凭据" description="登录后才能管理服务器执行凭据。" actions={backAction}>
        <section className="platform-page-card p-8 space-y-4">
          <p className="text-sm text-slate-500">当前未登录，无法管理服务器凭据。</p>
          <Link to="/auth/login" className="platform-page-primary-button inline-flex">去登录</Link>
        </section>
      </PlatformPageShell>
    );
  }

  return (
    <PlatformPageShell
      kicker="Admin Server Credentials"
      title="服务器凭据"
      description="管理服务器模式执行配置下的真实凭据、启停状态和健康检查结果。"
      actions={
        <div className="flex flex-wrap gap-2">
          {backAction}
          <button type="button" onClick={() => void loadData()} className="platform-page-action-link" disabled={loading}>
            <RefreshCcw size={16} />
            <span>刷新凭据</span>
          </button>
        </div>
      }
    >
      {error ? <section className="platform-page-card p-4 text-sm text-rose-600">{error}</section> : null}
      {message ? <section className="platform-page-card p-4 text-sm text-emerald-700">{message}</section> : null}

      <section className="platform-page-card p-6 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold text-slate-900">执行配置</h2>
            <p className="text-sm text-slate-500">选择 profile 后只查看该配置下的凭据。</p>
          </div>
          <ShieldCheck className="text-emerald-600" size={22} />
        </div>
        <select
          value={selectedProfileId ?? ""}
          onChange={(event) => {
            const value = event.target.value ? Number(event.target.value) : null;
            setSelectedProfileId(value);
            void loadData(value);
          }}
          className="w-full rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700"
        >
          <option value="">全部执行配置</option>
          {profiles.map((profile) => (
            <option key={profile.id} value={profile.id}>{profileLabel(profile)}</option>
          ))}
        </select>
      </section>

      <section className="platform-page-card p-6 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold text-slate-900">凭据表单</h2>
            <p className="text-sm text-slate-500">编辑时 credential / secret 留空表示保留原值，原始密钥不会回显。</p>
          </div>
          <button type="button" onClick={startCreate} className="platform-page-action-link">新增凭据</button>
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          <label className="space-y-1 text-sm text-slate-600">
            <span>执行配置</span>
            <select value={form.execution_profile_id || ""} onChange={(event) => patchForm({ execution_profile_id: Number(event.target.value) })} className="w-full rounded-lg border border-slate-200 px-3 py-2">
              <option value="">请选择</option>
              {profiles.map((profile) => (
                <option key={profile.id} value={profile.id}>{profile.display_name}</option>
              ))}
            </select>
          </label>
          <label className="space-y-1 text-sm text-slate-600">
            <span>标签</span>
            <input value={form.label} onChange={(event) => patchForm({ label: event.target.value })} className="w-full rounded-lg border border-slate-200 px-3 py-2" />
          </label>
          <label className="space-y-1 text-sm text-slate-600">
            <span>Provider</span>
            <select value={form.provider} onChange={(event) => patchForm({ provider: event.target.value === "anthropic" ? "anthropic" : "openai" })} className="w-full rounded-lg border border-slate-200 px-3 py-2">
              <option value="openai">openai</option>
              <option value="anthropic">anthropic</option>
            </select>
          </label>
          <label className="space-y-1 text-sm text-slate-600">
            <span>认证类型</span>
            <select value={form.auth_type} onChange={(event) => patchForm({ auth_type: event.target.value === "ak_sk" ? "ak_sk" : "api_key" })} className="w-full rounded-lg border border-slate-200 px-3 py-2">
              <option value="api_key">api_key</option>
              <option value="ak_sk">ak_sk</option>
            </select>
          </label>
          <label className="space-y-1 text-sm text-slate-600">
            <span>Credential</span>
            <input value={form.credential} onChange={(event) => patchForm({ credential: event.target.value })} placeholder={form.id ? "留空表示保留原值" : "输入 API Key / AK"} className="w-full rounded-lg border border-slate-200 px-3 py-2" />
          </label>
          <label className="space-y-1 text-sm text-slate-600">
            <span>Secret</span>
            <input value={form.secret} onChange={(event) => patchForm({ secret: event.target.value })} placeholder="留空表示保留原值" className="w-full rounded-lg border border-slate-200 px-3 py-2" />
          </label>
          <label className="space-y-1 text-sm text-slate-600">
            <span>Base URL</span>
            <input value={form.base_url} onChange={(event) => patchForm({ base_url: event.target.value })} className="w-full rounded-lg border border-slate-200 px-3 py-2" />
          </label>
          <label className="space-y-1 text-sm text-slate-600">
            <span>优先级</span>
            <input type="number" value={form.priority} onChange={(event) => patchForm({ priority: Number(event.target.value) || 0 })} className="w-full rounded-lg border border-slate-200 px-3 py-2" />
          </label>
        </div>
        <label className="inline-flex items-center gap-2 text-sm text-slate-600">
          <input type="checkbox" checked={form.enabled} onChange={(event) => patchForm({ enabled: event.target.checked })} />
          <span>启用凭据</span>
        </label>
        <button type="button" onClick={() => void submitForm()} disabled={saving || !form.execution_profile_id} className="platform-page-primary-button">
          {form.id === null ? "新增服务器凭据" : "保存服务器凭据"}
        </button>
      </section>

      <section className="platform-page-card p-6 space-y-4">
        <h2 className="text-lg font-semibold text-slate-900">凭据列表</h2>
        {loading ? (
          <p className="text-sm text-slate-500">正在读取服务器凭据...</p>
        ) : credentials.length === 0 ? (
          <p className="text-sm text-slate-500">当前没有服务器凭据。</p>
        ) : (
          <div className="space-y-3">
            {credentials.map((credential) => (
              <article key={credential.id} className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <p className="text-sm font-semibold text-slate-900">{credential.label || `credential #${credential.id}`}</p>
                    <p className="mt-1 text-xs text-slate-500">{profileLabel(profileById.get(credential.execution_profile_id))}</p>
                    <p className="mt-1 text-xs text-slate-500">{credential.provider} / {credential.auth_type} / priority {credential.priority}</p>
                    {credential.last_error_message ? <p className="mt-1 text-xs text-rose-600">{credential.last_error_code}: {credential.last_error_message}</p> : null}
                  </div>
                  <div className="text-right text-xs">
                    <p className={`font-semibold ${healthTone(credential.health_status)}`}>{credential.health_status}</p>
                    <p className="mt-1 text-slate-500">{credential.enabled ? "已启用" : "已禁用"}</p>
                    <p className="mt-1 text-slate-500">{formatCredentialTime(credential.last_checked_at)}</p>
                  </div>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <button type="button" onClick={() => startEdit(credential)} className="platform-page-action-link">编辑</button>
                  {credential.enabled ? (
                    <button type="button" onClick={() => void runCredentialAction(() => disableAdminServerCredential(credential.id), "服务器凭据已禁用。")} className="platform-page-action-link" disabled={saving}>禁用</button>
                  ) : (
                    <button type="button" onClick={() => void runCredentialAction(() => enableAdminServerCredential(credential.id), "服务器凭据已启用。")} className="platform-page-action-link" disabled={saving}>启用</button>
                  )}
                  <button type="button" onClick={() => void runCredentialAction(() => runAdminServerCredentialHealthCheck(credential.id), "服务器凭据健康检查已完成。")} className="platform-page-action-link" disabled={saving || !credential.enabled}>健康检查</button>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </PlatformPageShell>
  );
}

export default AdminServerCredentialsPage;
