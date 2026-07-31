// 本机能力体检卡片 —— OS / CPU / ilspycmd / dotnet 状态一眼可见。
//
// 初始 mount 调 sync 版本（0ms，填 OS / arch / CPU / ilspycmd）。
// 点 "Detect dotnet" 触发 full 版本（5s timeout 子进程探测 dotnet --version）。
//
// 缺工具时显示安装命令（可一键复制）和官方下载链接。

import { useEffect, useState } from "react";
import { Badge, Button, Card, KV, KVList, Notice } from "@/components/ui";
import { api } from "@/services/api";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { toActionableFailure } from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
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
    <Button size="sm" onClick={() => void copy()}>
      {copied ? "✓ Copied" : "Copy"}
    </Button>
  );
}

export function CapabilitiesCard() {
  const [caps, setCaps] = useState<LocalCapabilities | null>(null);
  const [error, setError] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void (async () => {
      try {
        const c = (await api.getLocalCapabilitiesSync()) as LocalCapabilities;
        setCaps(c);
      } catch (e: unknown) {
        setError(toActionableFailure(e));
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
      setError(toActionableFailure(e));
    } finally {
      setBusy(false);
    }
  }

  if (!__IS_TAURI__) {
    return (
      <Card
        eyebrow="local · capabilities"
        title="Local capabilities"
        subtitle="桌面端 only —— 本机环境检测（OS / CPU / ilspycmd / dotnet）。"
      />
    );
  }

  return (
    <Card
      eyebrow="local · capabilities"
      title="Local capabilities"
      actions={
        <Button size="sm" onClick={runFullDetect} disabled={busy}>
          {busy ? "Detecting…" : "Detect dotnet"}
        </Button>
      }
    >
      <ActionableErrorNotice failure={error} />

      {caps ? (
        <div className="space-y-3">
          <KVList>
            <KV k="OS / arch">
              <code>
                {caps.os}/{caps.arch}
              </code>
            </KV>
            <KV k="CPU cores">
              {caps.cpuCount > 0 ? caps.cpuCount : "(detection failed)"}
            </KV>
            <KV k="ilspycmd">
              {caps.ilspycmdFound ? (
                <span className="inline-flex items-center gap-2">
                  <Badge variant="ok">found</Badge>
                  {caps.ilspycmdPath && <code>{caps.ilspycmdPath}</code>}
                </span>
              ) : (
                <Badge variant="warn">未找到</Badge>
              )}
            </KV>
            <KV k="dotnet">
              {caps.dotnetVersion ? (
                <span className="inline-flex items-center gap-2">
                  <Badge variant="ok">{caps.dotnetVersion}</Badge>
                </span>
              ) : (
                <Badge variant="warn">未检测（点右上 Detect dotnet）</Badge>
              )}
            </KV>
          </KVList>

          {!caps.ilspycmdFound && (
            <Notice
              variant="warn"
              title="需要 ilspycmd —— Truth Snapshot 刷新反编译 sts2.dll 用"
            >
              <p style={{ marginBottom: "6px", color: "var(--ink-mute)" }}>
                前置 dotnet SDK 6+（参 dotnet 行），然后执行：
              </p>
              <div className="flex items-center gap-2 flex-wrap">
                <code>{ILSPYCMD_INSTALL}</code>
                <CopyButton text={ILSPYCMD_INSTALL} />
              </div>
              <p
                className="mt-2"
                style={{ color: "var(--ink-faint)", fontSize: "11px" }}
              >
                装完后点右上 "Detect dotnet" 或重启应用刷新检测。
              </p>
            </Notice>
          )}

          {caps.dotnetVersion === null && (
            <Notice
              variant="warn"
              title="未检测到 dotnet —— build_project / ilspycmd 都依赖"
            >
              <p>
                官方下载：
                <a href={DOTNET_DOWNLOAD} target="_blank" rel="noreferrer">
                  {DOTNET_DOWNLOAD}
                </a>
              </p>
              <p
                className="mt-1"
                style={{ color: "var(--ink-faint)", fontSize: "11px" }}
              >
                建议装 SDK 9.0（mod_template 默认 TargetFramework=net9.0）。
              </p>
            </Notice>
          )}

          {caps.warnings.length > 0 && (
            <Notice variant="warn" title="Warnings">
              <ul className="space-y-0.5">
                {caps.warnings.map((w, i) => (
                  <li key={i} style={{ fontSize: "12px" }}>⚠ {w}</li>
                ))}
              </ul>
            </Notice>
          )}
        </div>
      ) : (
        <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Detecting…</p>
      )}
    </Card>
  );
}
