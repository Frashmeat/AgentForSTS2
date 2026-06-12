// 首次运行引导横幅：检测关键就绪项，列出缺哪些 + 链到对应卡片。
// 全部绿了横幅消失（不持久化 dismiss —— 全绿就够清晰）。

import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Notice } from "@/components/ui";
import { api } from "@/services/api";
import type { HealthReport, LocalCapabilities } from "@/services/tauriApi";

interface CheckItem {
  ok: boolean;
  label: string;
  hint?: string;
  to?: string;
  href?: string;
}

export function FirstRunBanner() {
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [caps, setCaps] = useState<LocalCapabilities | null>(null);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void (async () => {
      try {
        setHealth((await api.getHealth()) as HealthReport);
      } catch {
        // 失败让其它检测继续
      }
      try {
        setCaps((await api.getLocalCapabilitiesSync()) as LocalCapabilities);
      } catch {
        // 同上
      }
    })();
  }, []);

  if (!__IS_TAURI__) return null;
  if (!health && !caps) return null;

  const items: CheckItem[] = [
    {
      ok: health?.readiness.llmConfigured ?? false,
      label: "LLM api_key 配置",
      hint: "去 System → 配置 页填 provider / model / base_url / api_key",
      to: "/system?tab=config",
    },
    {
      ok: caps?.ilspycmdFound ?? false,
      label: "ilspycmd 已安装（knowledge_refresh 需要）",
      hint: "Capabilities 卡片有一键复制 dotnet tool install -g ilspycmd",
    },
    {
      ok: (caps?.dotnetVersion?.length ?? 0) > 0,
      label: "dotnet SDK 已安装（build_project 需要）",
      hint: "点 Capabilities 的 Detect dotnet 触发检测；未装去 dotnet.microsoft.com 拿 SDK 9.0",
      href: "https://dotnet.microsoft.com/download/dotnet/9.0",
    },
    {
      ok: health?.readiness.activeProjectOpen ?? false,
      label: "工程文件夹已打开",
      hint: "Project 卡片创建新工程 / 打开已有工程",
    },
  ];

  const missing = items.filter((i) => !i.ok);
  if (missing.length === 0) return null;

  return (
    <Notice
      variant="warn"
      title={`快速上手 — 还缺 ${missing.length} 项配置`}
      className="mb-4"
    >
      <ul className="space-y-2 mt-1">
        {missing.map((it, i) => (
          <li key={i} className="flex items-start gap-3">
            <span style={{ color: "var(--gold)", lineHeight: 1.4 }}>○</span>
            <div className="flex-1 min-w-0">
              <p style={{ color: "var(--ink)" }}>{it.label}</p>
              {it.hint && (
                <p
                  className="mt-0.5"
                  style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}
                >
                  {it.hint}
                </p>
              )}
            </div>
            {it.to && (
              <Link to={it.to} className="btn btn-accent btn-sm shrink-0">
                Go →
              </Link>
            )}
            {it.href && (
              <a
                href={it.href}
                target="_blank"
                rel="noreferrer"
                className="btn btn-accent btn-sm shrink-0"
              >
                Open
              </a>
            )}
          </li>
        ))}
      </ul>
      <p
        className="mt-3"
        style={{ fontSize: "11px", color: "var(--ink-faint)" }}
      >
        配齐这些项后横幅会自动消失。全部 ✓ 的项已经满足，不再列出。
      </p>
    </Notice>
  );
}
