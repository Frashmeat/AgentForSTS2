// 本机能力体检卡片 —— OS / CPU / ilspycmd / dotnet 状态一眼可见。
//
// 初始 mount 调 sync 版本（0ms，填 OS / arch / CPU / ilspycmd）。
// 点 "Detect dotnet" 触发 full 版本（5s timeout 子进程探测 dotnet --version）。

import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { LocalCapabilities } from "@/services/tauriApi";

export function CapabilitiesCard() {
  const [caps, setCaps] = useState<LocalCapabilities | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void (async () => {
      try {
        const c = (await api.getLocalCapabilitiesSync()) as LocalCapabilities;
        setCaps(c);
      } catch (e: unknown) {
        setError(String(e));
      }
    })();
  }, []);

  async function runFullDetect() {
    setBusy(true);
    setError(null);
    try {
      const c = (await api.getLocalCapabilitiesFull()) as LocalCapabilities;
      setCaps(c);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Local capabilities</h2>
        <p className="text-muted text-sm">
          桌面端 only —— 本机环境检测（OS / CPU / ilspycmd / dotnet）。
        </p>
      </section>
    );
  }

  return (
    <section className="rounded border border-muted/30 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-medium">Local capabilities</h2>
        <button
          type="button"
          onClick={runFullDetect}
          disabled={busy}
          className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
        >
          {busy ? "Detecting…" : "Detect dotnet"}
        </button>
      </div>

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}

      {caps ? (
        <>
          <dl className="text-sm grid grid-cols-[140px_1fr] gap-x-2 gap-y-1">
            <dt className="text-muted">OS / arch</dt>
            <dd>
              <code>
                {caps.os}/{caps.arch}
              </code>
            </dd>
            <dt className="text-muted">CPU cores</dt>
            <dd>{caps.cpuCount > 0 ? caps.cpuCount : "(detection failed)"}</dd>
            <dt className="text-muted">ilspycmd</dt>
            <dd>
              {caps.ilspycmdFound ? (
                <span className="text-emerald-600">
                  ✓{" "}
                  {caps.ilspycmdPath && (
                    <code className="text-xs break-all">{caps.ilspycmdPath}</code>
                  )}
                </span>
              ) : (
                <span className="text-amber-600">未找到</span>
              )}
            </dd>
            <dt className="text-muted">dotnet</dt>
            <dd>
              {caps.dotnetVersion ? (
                <span className="text-emerald-600">✓ {caps.dotnetVersion}</span>
              ) : (
                <span className="text-amber-600">
                  未检测（点右上 "Detect dotnet"）
                </span>
              )}
            </dd>
          </dl>

          {caps.warnings.length > 0 && (
            <ul className="space-y-1 text-xs text-amber-700">
              {caps.warnings.map((w, i) => (
                <li key={i}>⚠ {w}</li>
              ))}
            </ul>
          )}
        </>
      ) : (
        <p className="text-muted text-sm">Detecting…</p>
      )}
    </section>
  );
}
