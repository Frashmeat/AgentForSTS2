// System 页：Tab 导航 → 配置 | 运维 | 开发。
// 配置 tab 始终显示输入框，Save 按钮即时保存 hot-reload。
// Knowledge card（路径+状态+刷新）整合在运维 tab 中。

import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { CapabilitiesCard } from "@/components/CapabilitiesCard";
import { CodegenCard } from "@/components/CodegenCard";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { FirstRunBanner } from "@/components/FirstRunBanner";
import { HealthCard } from "@/components/HealthCard";
import { RunsCard } from "@/components/RunsCard";
import { KnowledgeCard } from "@/components/KnowledgeCard";
import { LlmCard } from "@/components/LlmCard";
import { PlanningCard } from "@/components/PlanningCard";
import {
  Badge, Button, Card, Field, KV, KVList, Notice, PageHero,
} from "@/components/ui";
import { buildPatch, formFromSnapshot, type FormState } from "@/pages/systemForm";
import { api } from "@/services/api";
import type { SettingsSnapshot } from "@/services/tauriApi";

// ---------------------------------------------------------------------------
// TabBar
// ---------------------------------------------------------------------------

function TabBar({ active, onSelect }: { active: string; onSelect: (tab: string) => void }) {
  const tabs = [
    { id: "config", label: "配置" },
    { id: "ops", label: "运维" },
    { id: "devtools", label: "开发" },
  ];
  return (
    <div className="flex gap-1 mb-4" style={{ borderBottom: "1px solid var(--rule-soft)" }}>
      {tabs.map((t) => (
        <button key={t.id} onClick={() => onSelect(t.id)}
          data-testid={`system-tab-${t.id}`}
          className="px-4 py-2"
          style={{
            background: "none", border: "none", cursor: "pointer",
            fontSize: "13px", fontFamily: '"JetBrains Mono", monospace',
            color: active === t.id ? "var(--ink)" : "var(--ink-mute)",
            borderBottom: active === t.id ? "2px solid var(--accent)" : "2px solid transparent",
          }}>{t.label}</button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function StatusDot({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-2" style={{ fontSize: "13px" }}>
      <span className="inline-block" style={{
        width: 8, height: 8, borderRadius: "50%",
        background: ok ? "var(--jade)" : "var(--accent)",
      }} />{label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

const PAGESUBTITLES: Record<string, string> = {
  config: "所有配置均为输入框编辑，Save 保存即热替换",
  ops: "状态监控 · 知识库 · 任务队列 · 审计日志",
  devtools: "LLM prompt preview · plan validation · playground",
};

export function SystemPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const raw = searchParams.get("tab") ?? "ops";
  const validTabs = ["config", "ops", "devtools"] as const;
  const activeTab: string = validTabs.includes(raw as typeof validTabs[number]) ? raw : "ops";

  function selectTab(t: string) { setSearchParams({ tab: t }, { replace: true }); }

  const [snap, setSnap] = useState<SettingsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [opened, setOpened] = useState<string | null>(null);
  const [form, setForm] = useState<FormState | null>(null);
  const [savedMsg, setSavedMsg] = useState<string | null>(null);

  async function reload() {
    setBusy(true); setError(null);
    try {
      const s = (await api.getSettingsSnapshot()) as SettingsSnapshot;
      setSnap(s); setForm(formFromSnapshot(s));
    } catch (e: unknown) { setError(String(e)); }
    finally { setBusy(false); }
  }

  useEffect(() => { void reload(); }, []);

  async function handleOpen() {
    setOpened(null); setError(null);
    try { setOpened((await api.openConfigInEditor()) as string); }
    catch (e: unknown) { setError(String(e)); }
  }

  async function handleSave() {
    if (!form || !snap) return;
    const patch = buildPatch(form, snap);
    if (!patch.llm && !patch.image_gen && !patch.runtime_workstation && !patch.toolchain) {
      setSavedMsg("没有改动"); return;
    }
    setBusy(true); setError(null); setSavedMsg(null);
    try {
      const updated = (await api.saveSettingsPatch(patch)) as SettingsSnapshot;
      setSnap(updated); setForm(formFromSnapshot(updated));
      setSavedMsg("✓ 已保存并热替换");
    } catch (e: unknown) { setError(String(e)); }
    finally { setBusy(false); }
  }

  function renderActions() {
    return (
      <>
        <Button
          variant="success"
          onClick={() => void handleSave()}
          disabled={busy}
          data-testid="settings-save"
        >
          {busy ? "Saving…" : "Save"}
        </Button>
        <Button size="sm" onClick={() => void reload()} disabled={busy}>
          {busy ? "Loading…" : "Reload"}
        </Button>
      </>
    );
  }

  // -----------------------------------------------------------------------
  // config tab — always editable (LLM / ImageGen / Runtime)
  // -----------------------------------------------------------------------

  function renderConfigTab() {
    if (!__IS_TAURI__) {
      return <Notice variant="warn" title="桌面端 only">Web 模式下后端配置由部署方维护，不在前端编辑。桌面端启动以获得完整编辑能力。</Notice>;
    }

    return (
      <>
        {error && <div data-testid="settings-error"><Notice variant="error" title={`Error: ${error}`} /></div>}
        {savedMsg && <div data-testid="settings-saved"><Notice variant="ok" title={savedMsg} /></div>}
        {!snap && !error && <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Loading…</p>}

        {snap && form && (
          <>
            {/* ---- Config file card ---- */}
            <Card eyebrow="file · config.json" title="Config file" actions={
              <Button size="sm" variant="accent" onClick={() => void handleOpen()}>Open in OS editor</Button>
            }>
              <p style={{ fontSize: "12px" }} className="mb-3">
                <span style={{ color: "var(--ink-mute)" }}>Path </span>
                <code className="break-all">{snap.configPath ?? "<none — using built-in defaults>"}</code>
              </p>
              <div className="flex flex-wrap gap-4 mb-2">
                <StatusDot ok={snap.configLoaded} label="loaded" />
                <StatusDot ok={snap.configErrors.length === 0} label={`${snap.configErrors.length} validation error(s)`} />
              </div>
              {snap.configErrors.length > 0 && (
                <Notice variant="warn" title="Validation errors" className="mt-2">
                  <ul className="space-y-0.5">{snap.configErrors.map((e, i) => <li key={i} style={{ fontSize: "12px" }}>• {e}</li>)}</ul>
                </Notice>
              )}
              {opened && <p className="mt-2 break-all" style={{ fontSize: "11.5px", color: "var(--jade)" }}>
                Opened: <code>{opened}</code>（外部改完需点 Reload 让 UI 同步，或重启 app 让 figment 重载）
              </p>}
            </Card>

            {/* ---- LLM card ---- */}
            <Card eyebrow="provider · llm" title="llm">
              <div className="grid grid-cols-2 gap-3">
                <Field label="provider" hint="选择 LLM 协议">
                  <select value={form.llmProvider} onChange={(e) => setForm({ ...form, llmProvider: e.target.value })} className="input-mono">
                    <option value="openai">openai</option>
                    <option value="anthropic">anthropic</option>
                    <option value="openai_compatible">openai_compatible</option>
                    <option value="new_api">new_api</option>
                    <option value="one_api">one_api</option>
                    {!["openai","anthropic","openai_compatible","new_api","one_api",""].includes(form.llmProvider) && (
                      <option value={form.llmProvider}>{form.llmProvider}</option>
                    )}
                  </select>
                </Field>
                <Field label="model" hint="gpt-4o-mini / claude-sonnet-4-6 / deepseek-v4 …">
                  <input value={form.llmModel}
                    onChange={(e) => setForm({ ...form, llmModel: e.target.value })}
                    placeholder="gpt-4o-mini" className="input-mono" />
                </Field>
              </div>
              <Field label="base_url" hint="留空走官方端点">
                <input value={form.llmBaseUrl}
                  onChange={(e) => setForm({ ...form, llmBaseUrl: e.target.value })}
                  placeholder="https://api.openai.com" className="input-mono" />
              </Field>
              <Field label="api_key" hint="留空 = 不改">
                <input value={form.llmApiKey}
                  onChange={(e) => setForm({ ...form, llmApiKey: e.target.value, llmApiKeyTouched: true })}
                  placeholder={snap.llm.apiKeyConfigured ? "（未改动 — 保留原值）" : ""} className="input-mono" />
              </Field>
              <div className="mt-3">
                {snap.llm.apiKeyConfigured
                  ? <Badge variant="ok">api_key configured · all LLM tasks ready</Badge>
                  : <Badge variant="warn">api_key missing · LLM tasks disabled</Badge>}
              </div>
            </Card>

            {/* ---- ImageGen card ---- */}
            <Card eyebrow="provider · image_gen" title="image_gen">
              <div className="grid grid-cols-2 gap-3">
                <Field label="provider" hint="选择生图协议">
                  <select value={form.igProvider} onChange={(e) => setForm({ ...form, igProvider: e.target.value })} className="input-mono">
                    <option value="openai">openai</option>
                    <option value="new_api">new_api</option>
                    {!["openai","new_api",""].includes(form.igProvider) && (
                      <option value={form.igProvider}>{form.igProvider}</option>
                    )}
                  </select>
                </Field>
                <Field label="model" hint="dall-e-3 / nano-banana …">
                  <input value={form.igModel}
                    onChange={(e) => setForm({ ...form, igModel: e.target.value })}
                    placeholder="dall-e-3" className="input-mono" />
                </Field>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <Field label="base_url" hint="留空走官方端点">
                  <input value={form.igBaseUrl}
                    onChange={(e) => setForm({ ...form, igBaseUrl: e.target.value })}
                    placeholder="https://api.openai.com" className="input-mono" />
                </Field>
                <Field label="size" hint="标准图像尺寸">
                  <select value={form.igSize} onChange={(e) => setForm({ ...form, igSize: e.target.value })} className="input-mono">
                    <option value="">(默认 1024×1024)</option>
                    <option value="1024x1024">1024×1024 (1:1)</option>
                    <option value="1792x1024">1792×1024 (16:9)</option>
                    <option value="1024x1792">1024×1792 (9:16)</option>
                    <option value="512x512">512×512</option>
                  </select>
                </Field>
              </div>
              <Field label="protocol" hint="auto 从模型名推断">
                <select value={form.igProtocol} onChange={(e) => setForm({ ...form, igProtocol: e.target.value })} className="input-mono">
                  <option value="auto">auto (推荐)</option>
                  <option value="images_api">images_api (DALL-E)</option>
                  <option value="chat_completions">chat_completions (Nano Banana / Gemini)</option>
                </select>
              </Field>
              <Field label="api_key" hint="同 LLM 规则">
                <input value={form.igApiKey}
                  onChange={(e) => setForm({ ...form, igApiKey: e.target.value, igApiKeyTouched: true })}
                  placeholder={snap.imageGen.apiKeyConfigured ? "（未改动 — 保留原值）" : ""} className="input-mono" />
              </Field>
              <div className="mt-3">
                {snap.imageGen.apiKeyConfigured
                  ? <Badge variant="ok">api_key configured · asset_generate ready</Badge>
                  : <Badge variant="warn">api_key missing · asset_generate disabled</Badge>}
              </div>
            </Card>

            {/* ---- Runtime card ---- */}
            <Card eyebrow="runtime · editable" title="Runtime">
              <Field label="github_token" hint="提供后 Truth Snapshot 刷新的 BaseLib 下载走认证（5000 req/h），避免 GitHub 未认证限流">
                <input type="password" value={form.rtGithubToken}
                  onChange={(e) => setForm({ ...form, rtGithubToken: e.target.value, rtGithubTokenTouched: true })}
                  placeholder={snap.runtimeWorkstation.githubToken ? "（未改动 — 保留原值）" : ""} className="input-mono"
                  data-testid="github-token" />
              </Field>
              <div className="grid grid-cols-2 gap-2 mt-2">
                <KVList>
                  <KV k="host">{snap.runtimeWorkstation.host}</KV>
                  <KV k="port">{snap.runtimeWorkstation.port}</KV>
                </KVList>
                <KVList>
                  <KV k="mount_frontend">{String(snap.runtimeWorkstation.mountFrontend)}</KV>
                  <KV k="requires_database">{String(snap.runtimeWorkstation.requiresDatabase)}</KV>
                </KVList>
              </div>
              <p className="mt-2" style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
                host/port/CORS 需重启后生效；github_token 保存即热替换。
              </p>
            </Card>

            {/* ---- Local toolchain card ---- */}
            <Card eyebrow="local · build toolchain" title="Toolchain">
              <Field label="Godot executable" hint="保存时校验 Godot 4.5.1">
                <input
                  value={form.godotExePath}
                  onChange={(e) => setForm({ ...form, godotExePath: e.target.value })}
                  placeholder="I:/Godot/Godot_v4.5.1-stable_win64.exe"
                  className="input-mono"
                  data-testid="godot-exe-path"
                />
              </Field>
            </Card>

            {/* ---- Knowledge card ---- */}
            <ErrorBoundary label="Knowledge"><KnowledgeCard /></ErrorBoundary>
          </>
        )}
      </>
    );
  }

  // -----------------------------------------------------------------------
  // ops tab
  // -----------------------------------------------------------------------

  function renderOpsTab() {
    return (
      <>
        <FirstRunBanner />
        <div className="space-y-4">
          <ErrorBoundary label="Health"><HealthCard /></ErrorBoundary>
          <ErrorBoundary label="Capabilities"><CapabilitiesCard /></ErrorBoundary>
          <ErrorBoundary label="Runs"><RunsCard /></ErrorBoundary>
        </div>
      </>
    );
  }

  // -----------------------------------------------------------------------
  // devtools tab
  // -----------------------------------------------------------------------

  function renderDevToolsTab() {
    return (
      <div className="space-y-4">
        <ErrorBoundary label="Planning"><PlanningCard /></ErrorBoundary>
        <ErrorBoundary label="Codegen"><CodegenCard /></ErrorBoundary>
        <ErrorBoundary label="LLM"><LlmCard /></ErrorBoundary>
      </div>
    );
  }

  return (
    <div>
      <PageHero
        eyebrow={activeTab === "config" ? "settings · runtime config" : activeTab === "devtools" ? "devtools" : "operations · system"}
        title="System"
        subtitle={PAGESUBTITLES[activeTab]}
        actions={activeTab === "config" ? renderActions() : undefined}
      />
      <TabBar active={activeTab} onSelect={selectTab} />
      <div className="space-y-4">
        {activeTab === "config" ? renderConfigTab() : activeTab === "devtools" ? renderDevToolsTab() : renderOpsTab()}
      </div>
    </div>
  );
}
