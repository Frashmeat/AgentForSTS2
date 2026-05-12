// Settings 页：展示当前生效配置 + 表单 in-place 编辑 + hot-reload。
//
// 编辑流程：
//   1. 切到 "Edit" 模式 → 字段变可编辑
//   2. api_key 默认占位"(未改动 — 保留原值)"，输入 = 替换为新值
//   3. Save → 调 saveSettingsPatch（后端原子写 config.json + replace in-memory）
//   4. 成功 → 退回 view 模式，重新拉 snapshot 显示
//
// 复杂的 runtime / auth 段还是引导用户用 Open in OS editor 改文件。

import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type {
  SettingsPatch,
  SettingsSnapshot,
} from "@/services/tauriApi";

function StatusDot({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-sm">
      <span className={ok ? "text-emerald-600" : "text-amber-600"}>
        {ok ? "✓" : "○"}
      </span>
      <span>{label}</span>
    </span>
  );
}

interface FormState {
  llmProvider: string;
  llmModel: string;
  llmBaseUrl: string;
  llmApiKey: string;
  llmApiKeyTouched: boolean;
  igProvider: string;
  igModel: string;
  igBaseUrl: string;
  igSize: string;
  igApiKey: string;
  igApiKeyTouched: boolean;
}

function formFromSnapshot(s: SettingsSnapshot): FormState {
  return {
    llmProvider: s.llm.provider,
    llmModel: s.llm.model,
    llmBaseUrl: s.llm.baseUrl,
    llmApiKey: "",
    llmApiKeyTouched: false,
    igProvider: s.imageGen.provider,
    igModel: s.imageGen.model,
    igBaseUrl: s.imageGen.baseUrl,
    igSize: s.imageGen.size,
    igApiKey: "",
    igApiKeyTouched: false,
  };
}

function buildPatch(form: FormState, original: SettingsSnapshot): SettingsPatch {
  const patch: SettingsPatch = {};
  const llm: NonNullable<SettingsPatch["llm"]> = {};
  if (form.llmProvider !== original.llm.provider) llm.provider = form.llmProvider;
  if (form.llmModel !== original.llm.model) llm.model = form.llmModel;
  if (form.llmBaseUrl !== original.llm.baseUrl) llm.base_url = form.llmBaseUrl;
  if (form.llmApiKeyTouched) llm.api_key = form.llmApiKey;
  if (Object.keys(llm).length > 0) patch.llm = llm;

  const ig: NonNullable<SettingsPatch["image_gen"]> = {};
  if (form.igProvider !== original.imageGen.provider) ig.provider = form.igProvider;
  if (form.igModel !== original.imageGen.model) ig.model = form.igModel;
  if (form.igBaseUrl !== original.imageGen.baseUrl) ig.base_url = form.igBaseUrl;
  if (form.igSize !== original.imageGen.size) ig.size = form.igSize;
  if (form.igApiKeyTouched) ig.api_key = form.igApiKey;
  if (Object.keys(ig).length > 0) patch.image_gen = ig;
  return patch;
}

export function SettingsPage() {
  const [snap, setSnap] = useState<SettingsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [opened, setOpened] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [form, setForm] = useState<FormState | null>(null);
  const [savedMsg, setSavedMsg] = useState<string | null>(null);

  async function reload() {
    setBusy(true);
    setError(null);
    try {
      const s = (await api.getSettingsSnapshot()) as SettingsSnapshot;
      setSnap(s);
      setForm(formFromSnapshot(s));
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  async function handleOpen() {
    setOpened(null);
    setError(null);
    try {
      const p = await api.openConfigInEditor();
      setOpened(p as string);
    } catch (e: unknown) {
      setError(String(e));
    }
  }

  async function handleSave() {
    if (!form || !snap) return;
    const patch = buildPatch(form, snap);
    if (!patch.llm && !patch.image_gen) {
      setSavedMsg("没有改动");
      return;
    }
    setBusy(true);
    setError(null);
    setSavedMsg(null);
    try {
      const updated = (await api.saveSettingsPatch(patch)) as SettingsSnapshot;
      setSnap(updated);
      setForm(formFromSnapshot(updated));
      setEditing(false);
      setSavedMsg("✓ 已保存并热替换（无需重启）");
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function handleCancel() {
    if (snap) setForm(formFromSnapshot(snap));
    setEditing(false);
    setSavedMsg(null);
  }

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h1 className="text-2xl font-semibold mb-2">Settings</h1>
        <p className="text-muted text-sm">
          桌面端 only —— Web 模式下后端配置由部署方维护，不在前端编辑。
        </p>
      </section>
    );
  }

  return (
    <div className="space-y-4">
      <header className="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h1 className="text-2xl font-semibold">Settings</h1>
          <p className="text-muted text-sm">
            LLM / image_gen 可在 UI 内编辑（保存即热替换）；其它字段请用 Open in OS editor。
          </p>
        </div>
        <div className="flex items-center gap-2">
          {editing ? (
            <>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={busy}
                className="text-sm px-3 py-1 rounded border border-emerald-500/60 text-emerald-700 hover:bg-emerald-50/40 disabled:opacity-50"
              >
                {busy ? "Saving…" : "Save"}
              </button>
              <button
                type="button"
                onClick={handleCancel}
                disabled={busy}
                className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
              >
                Cancel
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                onClick={() => {
                  setEditing(true);
                  setSavedMsg(null);
                }}
                disabled={busy}
                className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
              >
                Edit
              </button>
              <button
                type="button"
                onClick={() => void reload()}
                disabled={busy}
                className="text-xs px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
              >
                {busy ? "Loading…" : "Reload"}
              </button>
            </>
          )}
        </div>
      </header>

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}
      {savedMsg && <p className="text-emerald-600 text-sm">{savedMsg}</p>}
      {!snap && !error && <p className="text-muted text-sm">Loading…</p>}

      {snap && form && (
        <>
          <section className="rounded border border-muted/30 p-4 space-y-2">
            <div className="flex items-center justify-between flex-wrap gap-2">
              <h2 className="text-lg font-medium">Config file</h2>
              <button
                type="button"
                onClick={() => void handleOpen()}
                className="text-xs px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10"
              >
                Open in OS editor
              </button>
            </div>
            <p className="text-xs">
              <span className="text-muted">Path: </span>
              <code className="break-all">
                {snap.configPath ?? "<none — using built-in defaults>"}
              </code>
            </p>
            <div className="flex flex-wrap gap-4">
              <StatusDot ok={snap.configLoaded} label="loaded" />
              <StatusDot
                ok={snap.configErrors.length === 0}
                label={`${snap.configErrors.length} validation error(s)`}
              />
            </div>
            {snap.configErrors.length > 0 && (
              <ul className="text-xs space-y-0.5 text-amber-700">
                {snap.configErrors.map((e, i) => (
                  <li key={i}>• {e}</li>
                ))}
              </ul>
            )}
            {opened && (
              <p className="text-xs text-emerald-600 break-all">
                Opened: <code>{opened}</code>（外部改完需点 Reload 让 UI 同步，
                或重启 app 让 figment 重载）
              </p>
            )}
          </section>

          <section className="rounded border border-muted/30 p-4 space-y-2">
            <h2 className="text-lg font-medium">LLM</h2>
            {editing ? (
              <div className="space-y-2 text-sm">
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">provider</span>
                  <input
                    value={form.llmProvider}
                    onChange={(e) =>
                      setForm({ ...form, llmProvider: e.target.value })
                    }
                    placeholder="anthropic / openai / openai-compat"
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">model</span>
                  <input
                    value={form.llmModel}
                    onChange={(e) =>
                      setForm({ ...form, llmModel: e.target.value })
                    }
                    placeholder="claude-sonnet-4-6 / gpt-4o ..."
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">base_url</span>
                  <input
                    value={form.llmBaseUrl}
                    onChange={(e) =>
                      setForm({ ...form, llmBaseUrl: e.target.value })
                    }
                    placeholder="https://api.anthropic.com 或代理 URL"
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">
                    api_key（留空 = 不改；输入则替换）
                  </span>
                  <input
                    type="password"
                    value={form.llmApiKey}
                    onChange={(e) =>
                      setForm({
                        ...form,
                        llmApiKey: e.target.value,
                        llmApiKeyTouched: true,
                      })
                    }
                    placeholder={
                      snap.llm.apiKeyConfigured
                        ? "(已配置 — 留空保持原值)"
                        : "(未配置)"
                    }
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
              </div>
            ) : (
              <dl className="text-sm grid grid-cols-[140px_1fr] gap-x-2 gap-y-1">
                <dt className="text-muted">provider</dt>
                <dd>
                  <code>{snap.llm.provider || "<empty>"}</code>
                </dd>
                <dt className="text-muted">model</dt>
                <dd>
                  <code>{snap.llm.model || "<empty>"}</code>
                </dd>
                <dt className="text-muted">base_url</dt>
                <dd className="break-all">
                  <code>{snap.llm.baseUrl || "<empty>"}</code>
                </dd>
                <dt className="text-muted">api_key</dt>
                <dd>
                  <code>{snap.llm.apiKeyMasked}</code>
                </dd>
              </dl>
            )}
            <StatusDot ok={snap.llm.apiKeyConfigured} label="api_key 已配置" />
          </section>

          <section className="rounded border border-muted/30 p-4 space-y-2">
            <h2 className="text-lg font-medium">image_gen</h2>
            {editing ? (
              <div className="space-y-2 text-sm">
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">provider</span>
                  <input
                    value={form.igProvider}
                    onChange={(e) =>
                      setForm({ ...form, igProvider: e.target.value })
                    }
                    placeholder="openai-compat / dalle …"
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">model</span>
                  <input
                    value={form.igModel}
                    onChange={(e) =>
                      setForm({ ...form, igModel: e.target.value })
                    }
                    placeholder="dall-e-3 / gpt-image-1 …"
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">base_url</span>
                  <input
                    value={form.igBaseUrl}
                    onChange={(e) =>
                      setForm({ ...form, igBaseUrl: e.target.value })
                    }
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">
                    size（1024x1024 / 1792x1024 / 512x512 …）
                  </span>
                  <input
                    value={form.igSize}
                    onChange={(e) =>
                      setForm({ ...form, igSize: e.target.value })
                    }
                    placeholder="1024x1024"
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="text-muted text-xs">api_key（同 LLM 规则）</span>
                  <input
                    type="password"
                    value={form.igApiKey}
                    onChange={(e) =>
                      setForm({
                        ...form,
                        igApiKey: e.target.value,
                        igApiKeyTouched: true,
                      })
                    }
                    placeholder={
                      snap.imageGen.apiKeyConfigured
                        ? "(已配置 — 留空保持原值)"
                        : "(未配置)"
                    }
                    className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                  />
                </label>
              </div>
            ) : (
              <dl className="text-sm grid grid-cols-[140px_1fr] gap-x-2 gap-y-1">
                <dt className="text-muted">provider</dt>
                <dd>
                  <code>{snap.imageGen.provider || "<empty>"}</code>
                </dd>
                <dt className="text-muted">model</dt>
                <dd>
                  <code>{snap.imageGen.model || "<empty>"}</code>
                </dd>
                <dt className="text-muted">base_url</dt>
                <dd className="break-all">
                  <code>{snap.imageGen.baseUrl || "<empty>"}</code>
                </dd>
                <dt className="text-muted">size</dt>
                <dd>
                  <code>{snap.imageGen.size || "<default 1024x1024>"}</code>
                </dd>
                <dt className="text-muted">api_key</dt>
                <dd>
                  <code>{snap.imageGen.apiKeyMasked}</code>
                </dd>
              </dl>
            )}
            <StatusDot
              ok={snap.imageGen.apiKeyConfigured}
              label="api_key 已配置（asset_generate 需要）"
            />
          </section>

          <section className="rounded border border-muted/30 p-4 space-y-2">
            <h2 className="text-lg font-medium">Runtime（只读）</h2>
            <div className="grid grid-cols-2 gap-3">
              {[
                { label: "Workstation (desktop)", rt: snap.runtimeWorkstation },
                { label: "Web (HTTP server)", rt: snap.runtimeWeb },
              ].map(({ label, rt }) => (
                <div
                  key={label}
                  className="border border-muted/20 rounded p-2 text-xs"
                >
                  <p className="font-medium mb-1">{label}</p>
                  <p>
                    <span className="text-muted">host:port: </span>
                    <code>
                      {rt.host}:{rt.port}
                    </code>
                  </p>
                  <p>
                    <span className="text-muted">mount_frontend: </span>
                    <span>{String(rt.mountFrontend)}</span>
                  </p>
                  <p>
                    <span className="text-muted">requires_database: </span>
                    <span>{String(rt.requiresDatabase)}</span>
                  </p>
                </div>
              ))}
            </div>
            <p className="text-xs text-muted">
              要改 host/port/CORS 用 Open in OS editor，UI 内只暴露 LLM /
              image_gen 字段。
            </p>
          </section>
        </>
      )}
    </div>
  );
}
