import type {
  BuildProjectResponse,
  PackageProjectResponse,
} from "../shared/api/workflow.ts";

export type BuildDeployAction = "deploy" | "build" | "package";
export type BuildDeployStage = "idle" | "running" | "done" | "error";

export interface BuildDeployState {
  stage: BuildDeployStage;
  action: BuildDeployAction | null;
  log: string[];
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

export function createIdleBuildDeployState(): BuildDeployState {
  return {
    stage: "idle",
    action: null,
    log: [],
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
    deployedTo: null,
    summary: null,
    errorMsg: null,
  };
}

export function appendBuildDeployLog(
  state: BuildDeployState,
  chunk: string,
): BuildDeployState {
  return {
    ...state,
    log: [...state.log, chunk],
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
    deployedTo: null,
    summary: result.summary,
    errorMsg: result.errorMsg,
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
