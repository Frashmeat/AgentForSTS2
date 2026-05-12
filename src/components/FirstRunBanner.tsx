// 首次运行引导横幅：检测关键就绪项，红色横幅列出缺哪些 + 链到对应卡片。
// 全部绿了横幅消失（不持久化 dismiss —— 全绿就够清晰）。

import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api } from "@/services/api";
import type {
  HealthReport,
  LocalCapabilities,
} from "@/services/tauriApi";

interface CheckItem {
  ok: boolean;
  label: string;
  hint?: string;
  /// 内部路由路径（react-router）
  to?: string;
  /// 外部链接
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
  if (!health && !caps) return null; // 还没拉到

  const items: CheckItem[] = [
    {
      ok: health?.readiness.llmConfigured ?? false,
      label: "LLM api_key 配置",
      hint: "去 Settings → LLM 区域填 provider / model / base_url / api_key",
      to: "/settings",
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
    <section className="rounded border border-amber-500/50 bg-amber-50/30 p-4 mb-4">
      <p className="text-amber-700 font-medium mb-2">
        快速上手 — 还缺 {missing.length} 项配置
      </p>
      <ul className="space-y-1.5 text-sm">
        {missing.map((it, i) => (
          <li key={i} className="flex items-start gap-2">
            <span className="text-amber-600">○</span>
            <div className="flex-1">
              <p>{it.label}</p>
              {it.hint && (
                <p className="text-xs text-muted">{it.hint}</p>
              )}
            </div>
            {it.to && (
              <Link
                to={it.to}
                className="text-xs px-2 py-0.5 rounded border border-accent/60 text-accent hover:bg-accent/10 shrink-0"
              >
                Go →
              </Link>
            )}
            {it.href && (
              <a
                href={it.href}
                target="_blank"
                rel="noreferrer"
                className="text-xs px-2 py-0.5 rounded border border-accent/60 text-accent hover:bg-accent/10 shrink-0"
              >
                Open
              </a>
            )}
          </li>
        ))}
      </ul>
      <p className="text-xs text-muted mt-2">
        配齐这些项后横幅会自动消失。全部✓的项已经满足，不再列出。
      </p>
    </section>
  );
}
