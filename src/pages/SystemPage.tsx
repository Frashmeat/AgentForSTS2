// System 页：Tab 导航 → 配置 | 运维 | 开发。
// 配置 tab 始终显示输入框，Save 按钮即时保存 hot-reload。

import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { AuditCard } from "@/components/AuditCard";
import { CapabilitiesCard } from "@/components/CapabilitiesCard";
import { CodegenCard } from "@/components/CodegenCard";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { FirstRunBanner } from "@/components/FirstRunBanner";
import { HealthCard } from "@/components/HealthCard";
import { JobsCard } from "@/components/JobsCard";
import { KnowledgeCard } from "@/components/KnowledgeCard";
import { LlmCard } from "@/components/LlmCard";
import { PlanningCard } from "@/components/PlanningCard";
import {
  Badge, Button, Card, Field, KV, KVList, Notice, PageHero,
} from "@/components/ui";
import { api } from "@/services/api";
import type { SettingsPatch, SettingsSnapshot } from "@/services/tauriApi";

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
  igProtocol: string;
  igApiKey: string;
  igApiKeyTouched: boolean;
  rtGithubToken: string;
  rtGithubTokenTouched: boolean;
  kSts2DllPath: string;
  kSts2DllPathTouched: boolean;
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
    igProtocol: s.imageGen.protocol || "auto",
    igApiKey: "",
    igApiKeyTouched: false,
    rtGithubToken: s.runtimeWorkstation.githubToken,
    rtGithubTokenTouched: false,
    kSts2DllPath: s.knowledge.sts2DllPath,
    kSts2DllPathTouched: false,
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
  if (form.igProtocol !== (original.imageGen.protocol || "auto")) ig.protocol = form.igProtocol;
  if (form.igApiKeyTouched) ig.api_key = form.igApiKey;
  if (Object.keys(ig).length > 0) patch.image_gen = ig;

  if (form.rtGithubTokenTouched) patch.runtime_workstation = { github_token: form.rtGithubToken };
  if (form.kSts2DllPathTouched) patch.knowledge = { sts2_dll_path: form.kSts2DllPath };
  return patch;
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
    if (!patch.llm && !patch.image_gen && !patch.runtime_workstation && !patch.knowledge) {
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
        <Button variant="success" onClick={() => void handleSave()} disabled={busy}>
          {busy ? "Saving…" : "Save"}
        </Button>
        <Button size="sm" onClick={() => void reload()} disabled={busy}>
          {busy ? "Loading…" : "Reload"}
        </Button>
      </>
    );
  }

  // -----------------------------------------------------------------------
  // config tab — always editable
  // -----------------------------------------------------------------------

  function renderConfigTab() {
    if (!__IS_TAURI__) {
      return <Notice variant="warn" title="桌面端 only">Web 模式下后端配置由部署方维护，不在前端编辑。桌面端启动以获得完整编辑能力。</Notice>;
    }

    return (
      <>
        {error && <Notice variant="error" title={`Error: ${error}`} />}
        {savedMsg && <Notice variant="ok" title={savedMsg} />}
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
                <Field label="provider" hint="下拉选择或直接输入">
                  <input value={form.llmProvider} list="llm-providers"
                    onChange={(e) => setForm({ ...form, llmProvider: e.target.value })}
                    className="input-mono" />
                  <datalist id="llm-providers">
                    <option value="openai" />
                    <option value="anthropic" />
                    <option value="openai_compatible" />
                    <option value="new_api" />
                    <option value="one_api" />
                  </datalist>
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
                <Field label="provider" hint="下拉选择或直接输入">
                  <input value={form.igProvider} list="ig-providers"
                    onChange={(e) => setForm({ ...form, igProvider: e.target.value })}
                    className="input-mono" />
                  <datalist id="ig-providers">
                    <option value="openai" />
                    <option value="new_api" />
                  </datalist>
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
              <Field label="github_token" hint="提供后 knowledge_refresh 的 baselib 下载走认证（5000 req/h），避免 GitHub 未认证限流">
                <input type="password" value={form.rtGithubToken}
                  onChange={(e) => setForm({ ...form, rtGithubToken: e.target.value, rtGithubTokenTouched: true })}
                  placeholder={snap.runtimeWorkstation.githubToken ? "（未改动 — 保留原值）" : ""} className="input-mono" />
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

            {/* ---- Knowledge card ---- */}
            <Card eyebrow="knowledge · paths" title="Knowledge">
              <Field label="sts2.dll 路径" hint="Slay the Spire 2 安装目录下的 sts2.dll 路径，用于反编译游戏代码生成知识库。保存后请在运维 tab → Knowledge 卡点击刷新。">
                <div className="flex gap-2">
                  <input value={form.kSts2DllPath}
                    onChange={(e) => setForm({ ...form, kSts2DllPath: e.target.value, kSts2DllPathTouched: true })}
                    placeholder={snap.knowledge.sts2DllPath || "e.g. J:/SteamLibrary/steamapps/common/Slay the Spire 2/data_sts2_windows_x86_64/sts2.dll"}
                    className="input-mono flex-1" />
                  <Button size="sm" onClick={async () => {
                    try {
                      const found = await api.discoverSts2Dll();
                      if (found) setForm({ ...form, kSts2DllPath: found, kSts2DllPathTouched: true });
                      else setError("未自动发现 sts2.dll，请手动填写路径");
                    } catch (e: unknown) { setError(`探测失败: ${String(e)}`); }
                  }}>🔍 Detect</Button>
                </div>
              </Field>
              <p className="mt-2" style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
                保存后请到 运维 tab → Knowledge 卡点击「刷新知识库」运行反编译。
              </p>
            </Card>
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
          <ErrorBoundary label="Knowledge"><KnowledgeCard /></ErrorBoundary>
          <ErrorBoundary label="Jobs"><JobsCard /></ErrorBoundary>
          <ErrorBoundary label="Audit"><AuditCard /></ErrorBoundary>
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
