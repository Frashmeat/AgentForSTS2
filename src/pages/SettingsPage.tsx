// Settings 页：展示当前生效配置（敏感字段脱敏） + 打开 OS 编辑器编辑。
// 当前不支持在 UI 内 in-place 编辑（hot-reload 还没做）；改完后需手动重启 app。

import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { SettingsSnapshot } from "@/services/tauriApi";

function StatusDot({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-sm">
      <span
        className={
          ok ? "text-emerald-600" : "text-amber-600"
        }
      >
        {ok ? "✓" : "○"}
      </span>
      <span>{label}</span>
    </span>
  );
}

export function SettingsPage() {
  const [snap, setSnap] = useState<SettingsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [opened, setOpened] = useState<string | null>(null);

  async function reload() {
    setBusy(true);
    setError(null);
    try {
      const s = (await api.getSettingsSnapshot()) as SettingsSnapshot;
      setSnap(s);
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
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">Settings</h1>
          <p className="text-muted text-sm">
            当前 runtime 配置只读视图。修改请打开文件，存档后重启应用。
          </p>
        </div>
        <button
          type="button"
          onClick={() => void reload()}
          disabled={busy}
          className="text-xs px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
        >
          {busy ? "Loading…" : "Reload"}
        </button>
      </header>

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}
      {!snap && !error && <p className="text-muted text-sm">Loading…</p>}

      {snap && (
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
              <p className="text-xs text-emerald-600">
                Opened: <code className="break-all">{opened}</code>（改完存档 → 重启
                app 让 figment 重新加载）
              </p>
            )}
          </section>

          <section className="rounded border border-muted/30 p-4 space-y-2">
            <h2 className="text-lg font-medium">LLM</h2>
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
            <StatusDot
              ok={snap.llm.apiKeyConfigured}
              label="api_key 已配置"
            />
          </section>

          <section className="rounded border border-muted/30 p-4 space-y-2">
            <h2 className="text-lg font-medium">image_gen</h2>
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
            <StatusDot
              ok={snap.imageGen.apiKeyConfigured}
              label="api_key 已配置（asset_generate 需要）"
            />
          </section>

          <section className="rounded border border-muted/30 p-4 space-y-2">
            <h2 className="text-lg font-medium">Runtime</h2>
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
          </section>

          <section className="text-xs text-muted">
            <p>
              修改方式：点上面"Open in OS editor"打开 config.json，改完存档 →
              Cmd/Ctrl+R 或重新打开应用让 figment 重新加载。Hot-reload 在 stage
              5+ 之后会做。
            </p>
          </section>
        </>
      )}
    </div>
  );
}
