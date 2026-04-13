import { Bug, LayoutDashboard, Sparkles, Wrench } from "lucide-react";

import type { WorkspaceNavItem } from "../../components/workspace/WorkspaceShell.tsx";
import type { WorkspaceTab } from "../platform-run/types.ts";

export function resolveWorkspaceTab(value: string | null): WorkspaceTab {
  switch (value) {
    case "batch":
    case "edit":
    case "log":
      return value;
    default:
      return "single";
  }
}

export function buildWorkspacePath(tab: WorkspaceTab): string {
  return tab === "single" ? "/" : `/?tab=${tab}`;
}

export function buildSettingsPath(returnTo: string): string {
  const nextSearch = new URLSearchParams({ returnTo });
  return `/settings?${nextSearch.toString()}`;
}

export const workspaceNavItems: WorkspaceNavItem<WorkspaceTab>[] = [
  {
    id: "single",
    label: "单资产",
    shortLabel: "单资产",
    description: "描述、生成、审批和构建单个资产。",
    icon: Sparkles,
  },
  {
    id: "batch",
    label: "Mod 规划",
    shortLabel: "规划",
    description: "批量规划多个资产，并跟踪每个条目的执行状态。",
    icon: LayoutDashboard,
  },
  {
    id: "edit",
    label: "修改 Mod",
    shortLabel: "修改",
    description: "分析现有项目结构，并让 Code Agent 执行改动。",
    icon: Wrench,
  },
  {
    id: "log",
    label: "崩溃分析",
    shortLabel: "日志",
    description: "读取最近日志并生成故障定位建议。",
    icon: Bug,
  },
];
