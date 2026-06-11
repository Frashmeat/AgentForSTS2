// 当前 active project 的全局状态。所有需要知道"用户打开了哪个工程"的
// 组件读这一个源，不再各自调用 currentProject()。
//
// 初始化：Tauri 模式下监听 project-changed 事件 + 启动时拉一次快照。

import { create } from "zustand";
import type { ProjectSnapshot } from "@/services/tauriApi";

interface ProjectState {
  project: ProjectSnapshot | null;
  setProject: (p: ProjectSnapshot | null) => void;
}

export const useProjectStore = create<ProjectState>((set) => ({
  project: null,
  setProject: (p) => set({ project: p }),
}));

// Tauri 下初始化：监听后端 project-changed 事件 + 拉取当前快照。
// Dynamic import justified: @tauri-apps/api/event 是 Tauri 专属模块，
// Web 构建中不存在对应的 IPC bridge。
if (__IS_TAURI__) {
  void (async () => {
    const [{ listen }, { currentProject }] = await Promise.all([
      import("@tauri-apps/api/event"),
      import("@/services/tauriApi"),
    ]);

    // 初始快照
    try {
      const snap = await currentProject();
      useProjectStore.getState().setProject(snap);
    } catch {
      // 没有已打开的工程，保持 null
    }

    // 后续变更通过 Tauri 事件推送
    await listen<ProjectSnapshot | null>("project-changed", (e) => {
      useProjectStore.getState().setProject(e.payload);
    });
  })();
}
