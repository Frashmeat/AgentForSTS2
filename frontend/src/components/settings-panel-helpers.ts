// SettingsPanel 用到的纯常量、tailwind class 字面量与纯函数 helpers。
// 无 React 依赖，便于测试和 import。

import type { PlatformExecutionProfile } from "../shared/api/platform.ts";
import type { MyServerPreferenceView } from "../shared/api/me.ts";

export const inputCls =
  "w-full bg-white border border-slate-200 rounded-lg px-3 py-1.5 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-amber-400 focus:ring-1 focus:ring-amber-100";
export const selectCls =
  "w-full bg-white border border-slate-200 rounded-lg px-3 py-1.5 text-sm text-slate-800 focus:outline-none focus:border-amber-400";
export const readonlyInputCls = `${inputCls} bg-slate-50 text-slate-500`;

export const PROVIDER_MODELS: Record<string, string[]> = {
  bfl: ["flux.2-flex", "flux.2-pro", "flux.2-klein", "flux.2-max", "flux.1.1-pro"],
  fal: ["flux.2-flex", "flux.2-pro", "flux.2-dev", "flux.2-schnell"],
  volcengine: ["doubao-seedream-3-0-t2i-250415", "doubao-seedream-3-0-1-5b-t2i-250616"],
  wanxiang: [],
};

const KNOWLEDGE_UPDATE_STEP_PROGRESS: Array<{ pattern: RegExp; progress: number }> = [
  { pattern: /初始化更新任务|准备启动知识库更新任务|等待开始/, progress: 8 },
  { pattern: /读取当前游戏版本/, progress: 18 },
  { pattern: /反编译游戏源码/, progress: 48 },
  { pattern: /读取 Baselib latest release/, progress: 62 },
  { pattern: /下载 Baselib\.dll/, progress: 78 },
  { pattern: /反编译 Baselib/, progress: 92 },
  { pattern: /更新完成/, progress: 100 },
  { pattern: /更新失败/, progress: 100 },
];

export function getKnowledgeUpdateProgress(step: string, busy: boolean): number {
  if (!step.trim()) {
    return busy ? 8 : 0;
  }

  for (const item of KNOWLEDGE_UPDATE_STEP_PROGRESS) {
    if (item.pattern.test(step)) {
      return item.progress;
    }
  }

  return busy ? 24 : 100;
}

export function pickInitialServerProfileId(
  profiles: PlatformExecutionProfile[],
  preference: MyServerPreferenceView | null,
): number | null {
  if (
    preference?.default_execution_profile_id !== null &&
    typeof preference?.default_execution_profile_id !== "undefined"
  ) {
    const preferredProfile = profiles.find((profile) => profile.id === preference.default_execution_profile_id);
    return preferredProfile?.available ? preferredProfile.id : null;
  }
  return (
    profiles.find((profile) => profile.available && profile.recommended)?.id ??
    profiles.find((profile) => profile.available)?.id ??
    null
  );
}
