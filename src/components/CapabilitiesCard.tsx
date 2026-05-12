// 本机能力体检卡片 —— OS / CPU / ilspycmd / dotnet 状态一眼可见。
//
// 初始 mount 调 sync 版本（0ms，填 OS / arch / CPU / ilspycmd）。
// 点 "Detect dotnet" 触发 full 版本（5s timeout 子进程探测 dotnet --version）。
//
// 缺工具时显示安装命令（可一键复制）和官方下载链接。

import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { LocalCapabilities } from "@/services/tauriApi";

const ILSPYCMD_INSTALL = "dotnet tool install -g ilspycmd";
const DOTNET_DOWNLOAD = "https://dotnet.microsoft.com/download/dotnet/9.0";

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // 部分 webview / 浏览器无 clipboard API；用户手动选中即可
    }
  }
  return (
    <button
      type="button"
      onClick={() => void copy()}
      className="text-xs px-2 py-0.5 rounded border border-muted/40 hover:bg-muted/10 ml-2"
    >
      {copied ? "✓ Copied" : "Copy"}
    </button>
  );
}

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

          {/* 缺 ilspycmd：给出安装命令 + 复制按钮（前置：dotnet SDK 6+） */}
          {!caps.ilspycmdFound && (
            <div className="rounded border border-amber-500/30 bg-amber-50/20 p-2 text-xs">
              <p className="text-amber-700 font-medium mb-1">
                需要 ilspycmd —— knowledge_refresh 反编译 sts2.dll 用
              </p>
              <p className="text-muted mb-1">
                前置 dotnet SDK 6+（参 dotnet 行），然后执行：
              </p>
              <div className="flex items-center">
                <code className="px-2 py-1 bg-muted/10 rounded font-mono">
                  {ILSPYCMD_INSTALL}
                </code>
                <CopyButton text={ILSPYCMD_INSTALL} />
              </div>
              <p className="text-muted mt-1">
                装完后点右上 "Detect dotnet" 或重启应用刷新检测。
              </p>
            </div>
          )}

          {/* 缺 dotnet：给下载链接 + 注：build_project / ilspycmd 都依赖它 */}
          {caps.dotnetVersion === null && (
            <div className="rounded border border-amber-500/30 bg-amber-50/20 p-2 text-xs">
              <p className="text-amber-700 font-medium mb-1">
                未检测到 dotnet —— build_project / ilspycmd 都依赖
              </p>
              <p>
                <span className="text-muted">官方下载：</span>
                <a
                  href={DOTNET_DOWNLOAD}
                  target="_blank"
                  rel="noreferrer"
                  className="text-accent underline break-all"
                >
                  {DOTNET_DOWNLOAD}
                </a>
              </p>
              <p className="text-muted mt-1">
                建议装 SDK 9.0（mod_template 默认 TargetFramework=net9.0）。
              </p>
            </div>
          )}

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
