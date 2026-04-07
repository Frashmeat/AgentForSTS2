import type {
  BuildProjectResponse,
  PackageProjectResponse,
} from "../shared/api/workflow.ts";
import {
  appendWorkflowLogEntry,
  resolveNextWorkflowModel,
  type WorkflowLogEntry,
} from "../shared/workflowLog.ts";

export type BuildDeployAction = "deploy" | "build" | "package";
export type BuildDeployStage = "idle" | "running" | "done" | "error";

export interface BuildDeployState {
  stage: BuildDeployStage;
  action: BuildDeployAction | null;
  log: string[];
  logEntries: WorkflowLogEntry[];
  currentModel: string | null;
  deployedTo: string | null;
  summary: string | null;
  errorMsg: string | null;
}

export interface BuildDeployActionResult {
  stage: "done" | "error";
  summary: string | null;
  log: string[];
  errorMsg: string | null;
}

export interface BuildDeployCompletionView {
  tone: "success" | "warning";
  title: string;
  detail: string | null;
  detailMonospace: boolean;
  showOpenSettings: boolean;
}

export function createIdleBuildDeployState(): BuildDeployState {
  return {
    stage: "idle",
    action: null,
    log: [],
    logEntries: [],
    currentModel: null,
    deployedTo: null,
    summary: null,
    errorMsg: null,
  };
}

export function startBuildDeployAction(action: BuildDeployAction): BuildDeployState {
  return {
    stage: "running",
    action,
    log: [],
    logEntries: [],
    currentModel: null,
    deployedTo: null,
    summary: null,
    errorMsg: null,
  };
}

export function appendBuildDeployLog(
  state: BuildDeployState,
  entry: WorkflowLogEntry,
): BuildDeployState {
  return {
    ...state,
    log: [...state.log, entry.text],
    logEntries: appendWorkflowLogEntry(state.logEntries, entry),
    currentModel: resolveNextWorkflowModel(state.currentModel, entry),
  };
}

export function normalizeBuildOutputLines(output: string): string[] {
  return output
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean);
}

export function finalizeBuildProjectResult(
  response: BuildProjectResponse,
): BuildDeployActionResult {
  const log = normalizeBuildOutputLines(response.output);
  if (response.success) {
    return {
      stage: "done",
      summary: "构建成功",
      log,
      errorMsg: null,
    };
  }
  return {
    stage: "error",
    summary: null,
    log,
    errorMsg: response.output || "构建失败",
  };
}

export function finalizePackageProjectResult(
  response: PackageProjectResponse,
): BuildDeployActionResult {
  if (response.success) {
    return {
      stage: "done",
      summary: "打包成功",
      log: [],
      errorMsg: null,
    };
  }
  return {
    stage: "error",
    summary: null,
    log: [],
    errorMsg: "打包失败",
  };
}

export function finalizeDeployResult(
  state: BuildDeployState,
  deployedTo: string | null,
): BuildDeployState {
  return {
    ...state,
    stage: "done",
    deployedTo,
    summary: deployedTo ? "已部署" : "构建成功",
    errorMsg: null,
  };
}

export function failBuildDeployAction(
  state: BuildDeployState,
  errorMsg: string,
): BuildDeployState {
  return {
    ...state,
    stage: "error",
    deployedTo: null,
    summary: null,
    errorMsg,
  };
}

export function applyBuildDeployActionResult(
  state: BuildDeployState,
  result: BuildDeployActionResult,
): BuildDeployState {
  return {
    ...state,
    stage: result.stage,
    log: result.log,
    logEntries: [],
    currentModel: null,
    deployedTo: null,
    summary: result.summary,
    errorMsg: result.errorMsg,
  };
}

export function describeBuildDeployRunningMessage(action: BuildDeployAction): string {
  if (action === "deploy") {
    return "Code Agent 构建中（含 .pck 导出）…";
  }
  return `${describeBuildDeployAction(action)}中…`;
}

export function describeBuildDeployCompletionView(
  state: BuildDeployState,
): BuildDeployCompletionView | null {
  if (state.stage !== "done" || !state.action) {
    return null;
  }

  if (state.action === "deploy" && state.deployedTo) {
    return {
      tone: "success",
      title: "已部署",
      detail: state.deployedTo,
      detailMonospace: true,
      showOpenSettings: false,
    };
  }

  if (state.action === "deploy") {
    return {
      tone: "warning",
      title: "构建成功，未自动部署",
      detail: "在设置中配置 STS2 游戏路径后可自动复制到 Mods 文件夹",
      detailMonospace: false,
      showOpenSettings: true,
    };
  }

  return {
    tone: "success",
    title: state.summary ?? "执行成功",
    detail:
      state.action === "build"
        ? "已完成项目构建，可继续部署或排查构建输出。"
        : "已完成项目打包。",
    detailMonospace: false,
    showOpenSettings: false,
  };
}

export function describeBuildDeployAction(action: BuildDeployAction): string {
  switch (action) {
    case "deploy":
      return "构建并部署";
    case "build":
      return "仅构建";
    case "package":
      return "仅打包";
  }
}
