// 工作流状态 — 跨路由持久化 running job ID。
// 解决"生成中途切换页面，回来时 UI 永远停在等待态"的问题。
//
// SingleAssetWorkflowCard 提交 job 时写 store，job 完成/失败时清空。
// 组件挂载时读 store — 若有 in-flight job，用 getJob 查询当前状态并重建 UI。

import { create } from "zustand";

interface WorkflowState {
  /** 正在运行的 plan job ID。null = 无 */
  planJobId: string | null;
  setPlanJobId: (id: string | null) => void;
  /** 正在运行的 code/asset-generate job ID。null = 无 */
  codeJobId: string | null;
  setCodeJobId: (id: string | null) => void;
}

export const useWorkflowStore = create<WorkflowState>((set) => ({
  planJobId: null,
  setPlanJobId: (id) => set({ planJobId: id }),
  codeJobId: null,
  setCodeJobId: (id) => set({ codeJobId: id }),
}));
