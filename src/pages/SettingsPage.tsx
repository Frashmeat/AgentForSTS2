// Settings 页：展示当前生效配置 + 表单 in-place 编辑 + hot-reload。
//
// 编辑流程：
//   1. 切到 "Edit" 模式 → 字段变可编辑
//   2. api_key 默认占位"(未改动 — 保留原值)"，输入 = 替换为新值
//   3. Save → 调 saveSettingsPatch（后端原子写 config.json + replace in-memory）
//   4. 成功 → 退回 view 模式，重新拉 snapshot 显示
//
// 所有 config.json 字段都在此可编辑（llm / image_gen / runtime_workstation / knowledge）。

import { useEffect, useState } from "react";
import {
  Badge,
  Button,
  Card,
  Field,
  KV,
  KVList,
  Notice,
  PageHero,
} from "@/components/ui";
import { api } from "@/services/api";
import type { SettingsPatch, SettingsSnapshot } from "@/services/tauriApi";

function StatusDot({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-2" style={{ fontSize: "13px" }}>
      <span
        className="inline-block"
        style={{
          width: 8,
          height: 8,
          borderRadius: "50%",
          background: ok ? "var(--jade)" : "var(--accent)",
        }}
      />
      {label}
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

  if (form.rtGithubTokenTouched) {
    patch.runtime_workstation = { github_token: form.rtGithubToken };
  }
  if (form.kSts2DllPathTouched) {
    patch.knowledge = { sts2_dll_path: form.kSts2DllPath };
  }
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
    if (!patch.llm && !patch.image_gen && !patch.runtime_workstation && !patch.knowledge) {
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
      <div>
        <PageHero
          eyebrow="settings · config"
          title="Settings"
          subtitle="桌面端 only —— Web 模式下后端配置由部署方维护，不在前端编辑。"
        />
      </div>
    );
  }

  return (
    <div>
      <PageHero
        eyebrow="settings · runtime config"
        title="Settings"
        subtitle="所有运行时配置均可在 UI 编辑（保存即热替换）"
        actions={
          editing ? (
            <>
              <Button
                variant="success"
                onClick={() => void handleSave()}
                disabled={busy}
              >
                {busy ? "Saving…" : "Save"}
              </Button>
              <Button onClick={handleCancel} disabled={busy}>
                Cancel
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="accent"
                onClick={() => {
                  setEditing(true);
                  setSavedMsg(null);
                }}
                disabled={busy}
              >
                Edit
              </Button>
              <Button size="sm" onClick={() => void reload()} disabled={busy}>
                {busy ? "Loading…" : "Reload"}
              </Button>
            </>
          )
        }
      />

      <div className="space-y-4">
        {error && <Notice variant="error" title={`Error: ${error}`} />}
        {savedMsg && <Notice variant="ok" title={savedMsg} />}
        {!snap && !error && (
          <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Loading…</p>
        )}

        {snap && form && (
          <>
            <Card
              eyebrow="file · config.json"
              title="Config file"
              actions={
                <Button size="sm" variant="accent" onClick={() => void handleOpen()}>
                  Open in OS editor
                </Button>
              }
            >
              <p style={{ fontSize: "12px" }} className="mb-3">
                <span style={{ color: "var(--ink-mute)" }}>Path </span>
                <code className="break-all">
                  {snap.configPath ?? "<none — using built-in defaults>"}
                </code>
              </p>
              <div className="flex flex-wrap gap-4 mb-2">
                <StatusDot ok={snap.configLoaded} label="loaded" />
                <StatusDot
                  ok={snap.configErrors.length === 0}
                  label={`${snap.configErrors.length} validation error(s)`}
                />
              </div>
              {snap.configErrors.length > 0 && (
                <Notice variant="warn" title="Validation errors" className="mt-2">
                  <ul className="space-y-0.5">
                    {snap.configErrors.map((e, i) => (
                      <li key={i} style={{ fontSize: "12px" }}>• {e}</li>
                    ))}
                  </ul>
                </Notice>
              )}
              {opened && (
                <p
                  className="mt-2 break-all"
                  style={{ fontSize: "11.5px", color: "var(--jade)" }}
                >
                  Opened: <code>{opened}</code>（外部改完需点 Reload 让 UI 同步，
                  或重启 app 让 figment 重载）
                </p>
              )}
            </Card>

            <Card eyebrow="provider · llm" title="llm">
              {editing ? (
                <>
                  <div className="grid grid-cols-2 gap-3">
                    <Field label="provider" hint="anthropic / openai / new_api …">
                      <input
                        value={form.llmProvider}
                        onChange={(e) =>
                          setForm({ ...form, llmProvider: e.target.value })
                        }
                        placeholder="openai"
                        className="input-mono"
                      />
                    </Field>
                    <Field label="model" hint="gpt-4o-mini / claude-sonnet-4-6 / deepseek-v4 …">
                      <input
                        value={form.llmModel}
                        onChange={(e) =>
                          setForm({ ...form, llmModel: e.target.value })
                        }
                        placeholder="gpt-4o-mini"
                        className="input-mono"
                      />
                    </Field>
                  </div>
                  <Field label="base_url" hint="留空走官方端点">
                    <input
                      value={form.llmBaseUrl}
                      onChange={(e) =>
                        setForm({ ...form, llmBaseUrl: e.target.value })
                      }
                      placeholder="https://api.openai.com"
                      className="input-mono"
                    />
                  </Field>
                  <Field
                    label="api_key"
                    hint="留空 = 不改"
                  >
                    <input
                      value={form.llmApiKey}
                      onChange={(e) => {
                        setForm({
                          ...form,
                          llmApiKey: e.target.value,
                          llmApiKeyTouched: true,
                        });
                      }}
                      placeholder={snap.llm.apiKeyConfigured ? "（未改动 — 保留原值）" : ""}
                      className="input-mono"
                    />
                  </Field>
                </>
              ) : (
                <KVList>
                  <KV k="provider">
                    <code>{snap.llm.provider || "<default>"}</code>
                  </KV>
                  <KV k="model">
                    <code>{snap.llm.model || "<default>"}</code>
                  </KV>
                  <KV k="base_url">
                    <code>{snap.llm.baseUrl || "<official endpoint>"}</code>
                  </KV>
                  <KV k="api_key">
                    <code>{snap.llm.apiKeyMasked}</code>
                  </KV>
                </KVList>
              )}
              <div className="mt-3">
                {snap.llm.apiKeyConfigured ? (
                  <Badge variant="ok">api_key configured · all LLM tasks ready</Badge>
                ) : (
                  <Badge variant="warn">api_key missing · LLM tasks disabled</Badge>
                )}
              </div>
            </Card>

            <Card eyebrow="provider · image_gen" title="image_gen">
              {editing ? (
                <>
                  <div className="grid grid-cols-2 gap-3">
                    <Field label="provider" hint="openai / new_api …">
                      <input
                        value={form.igProvider}
                        onChange={(e) =>
                          setForm({ ...form, igProvider: e.target.value })
                        }
                        placeholder="openai"
                        className="input-mono"
                      />
                    </Field>
                    <Field label="model" hint="dall-e-3 / nano-banana …">
                      <input
                        value={form.igModel}
                        onChange={(e) =>
                          setForm({ ...form, igModel: e.target.value })
                        }
                        placeholder="dall-e-3"
                        className="input-mono"
                      />
                    </Field>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <Field label="base_url" hint="留空走官方端点">
                      <input
                        value={form.igBaseUrl}
                        onChange={(e) =>
                          setForm({ ...form, igBaseUrl: e.target.value })
                        }
                        placeholder="https://api.openai.com"
                        className="input-mono"
                      />
                    </Field>
                    <Field
                      label="size"
                      hint="1024x1024 / 1792x1024 / 512x512 …"
                    >
                      <input
                        value={form.igSize}
                        onChange={(e) =>
                          setForm({ ...form, igSize: e.target.value })
                        }
                        placeholder="1024x1024"
                        className="input-mono"
                      />
                    </Field>
                  </div>
                  <Field
                    label="protocol"
                    hint="auto 从模型名推断；images_api 走 DALL-E 标准；chat_completions 走 Gemini/Nano Banana"
                  >
                    <select
                      value={form.igProtocol}
                      onChange={(e) =>
                        setForm({ ...form, igProtocol: e.target.value })
                      }
                      className="input-mono"
                    >
                      <option value="auto">auto (推荐)</option>
                      <option value="images_api">images_api (DALL-E)</option>
                      <option value="chat_completions">chat_completions (Nano Banana / Gemini)</option>
                    </select>
                  </Field>
                  <Field
                    label="api_key"
                    hint="同 LLM 规则"
                  >
                    <input
                      value={form.igApiKey}
                      onChange={(e) => {
                        setForm({
                          ...form,
                          igApiKey: e.target.value,
                          igApiKeyTouched: true,
                        });
                      }}
                      placeholder={snap.imageGen.apiKeyConfigured ? "（未改动 — 保留原值）" : ""}
                      className="input-mono"
                    />
                  </Field>
                </>
              ) : (
                <KVList>
                  <KV k="provider">
                    <code>{snap.imageGen.provider || "<default>"}</code>
                  </KV>
                  <KV k="model">
                    <code>{snap.imageGen.model || "<default>"}</code>
                  </KV>
                  <KV k="base_url">
                    <code>{snap.imageGen.baseUrl || "<official endpoint>"}</code>
                  </KV>
                  <KV k="size">
                    <code>{snap.imageGen.size || "<default 1024x1024>"}</code>
                  </KV>
                  <KV k="protocol">
                    <code>{snap.imageGen.protocol || "auto"}</code>
                  </KV>
                  <KV k="api_key">
                    <code>{snap.imageGen.apiKeyMasked}</code>
                  </KV>
                </KVList>
              )}
              <div className="mt-3">
                {snap.imageGen.apiKeyConfigured ? (
                  <Badge variant="ok">api_key configured · asset_generate ready</Badge>
                ) : (
                  <Badge variant="warn">api_key missing · asset_generate disabled</Badge>
                )}
              </div>
            </Card>

            <Card eyebrow="runtime · editable" title="Runtime">
              {editing ? (
                <Field
                  label="github_token"
                  hint="提供后 knowledge_refresh 的 baselib 下载走认证（5000 req/h），避免 GitHub 未认证限流"
                >
                  <input
                    type="password"
                    value={form.rtGithubToken}
                    onChange={(e) => {
                      setForm({
                        ...form,
                        rtGithubToken: e.target.value,
                        rtGithubTokenTouched: true,
                      });
                    }}
                    placeholder={snap.runtimeWorkstation.githubToken ? "（未改动 — 保留原值）" : ""}
                    className="input-mono"
                  />
                </Field>
              ) : (
                <KVList>
                  <KV k="github_token">
                    <code>
                      {snap.runtimeWorkstation.githubToken || "<not set>"}
                    </code>
                  </KV>
                  <KV k="host:port">
                    <code>
                      {snap.runtimeWorkstation.host}:{snap.runtimeWorkstation.port}
                    </code>
                  </KV>
                  <KV k="mount_frontend">
                    {String(snap.runtimeWorkstation.mountFrontend)}
                  </KV>
                  <KV k="requires_database">
                    {String(snap.runtimeWorkstation.requiresDatabase)}
                  </KV>
                </KVList>
              )}
              <p className="mt-2" style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
                host/port/CORS 需重启后生效；github_token 保存即热替换。
              </p>
            </Card>

            <Card eyebrow="knowledge · paths" title="Knowledge">
              {editing ? (
                <>
                  <Field
                    label="sts2.dll 路径"
                    hint="Slay the Spire 2 安装目录下的 sts2.dll 路径，用于反编译游戏代码生成知识库"
                  >
                    <div className="flex gap-2">
                      <input
                        value={form.kSts2DllPath}
                        onChange={(e) => {
                          setForm({
                            ...form,
                            kSts2DllPath: e.target.value,
                            kSts2DllPathTouched: true,
                          });
                        }}
                        placeholder={snap.knowledge.sts2DllPath || "e.g. J:/SteamLibrary/steamapps/common/Slay the Spire 2/data_sts2_windows_x86_64/sts2.dll"}
                        className="input-mono flex-1"
                      />
                      <Button
                        size="sm"
                        onClick={async () => {
                          try {
                            const found = await api.discoverSts2Dll();
                            if (found) {
                              setForm({
                                ...form,
                                kSts2DllPath: found,
                                kSts2DllPathTouched: true,
                              });
                            } else {
                              setError("未自动发现 sts2.dll，请手动填写路径");
                            }
                          } catch (e: unknown) {
                            setError(`探测失败: ${String(e)}`);
                          }
                        }}
                      >
                        🔍 Detect
                      </Button>
                    </div>
                  </Field>
                </>
              ) : (
                <KVList>
                  <KV k="sts2.dll">
                    <code className="break-all">
                      {snap.knowledge.sts2DllPath || "<not set — knowledge_refresh 不可用>"}
                    </code>
                  </KV>
                </KVList>
              )}
              <p className="mt-2" style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
                配置后请到 Dashboard → Knowledge 卡点击「刷新知识库」运行反编译。
              </p>
            </Card>
          </>
        )}
      </div>
    </div>
  );
}
