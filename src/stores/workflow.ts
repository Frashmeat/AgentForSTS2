// 工作流状态 — 跨路由持久化 running run ID。
// 解决"生成中途切换页面，回来时 UI 永远停在等待态"的问题。
//
// SingleAssetWorkflowCard 提交 run 时写 store，run 完成/失败时清空。
// 组件挂载时读 store — 若有 in-flight run，用 getRun 查询当前状态并重建 UI。

import { create } from "zustand";

interface WorkflowState {
  /** 正在运行的 plan run ID。null = 无 */
  planRunId: string | null;
  setPlanRunId: (id: string | null) => void;
  /** 正在运行的 code/asset-generate run ID。null = 无 */
  codeRunId: string | null;
  setCodeRunId: (id: string | null) => void;
}

export const useWorkflowStore = create<WorkflowState>((set) => ({
  planRunId: null,
  setPlanRunId: (id) => set({ planRunId: id }),
  codeRunId: null,
  setCodeRunId: (id) => set({ codeRunId: id }),
}));
