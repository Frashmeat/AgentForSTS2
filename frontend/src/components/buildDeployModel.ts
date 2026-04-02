import type {
  BuildProjectResponse,
  PackageProjectResponse,
} from "../shared/api/workflow.ts";

export type BuildDeployAction = "deploy" | "build" | "package";

export interface BuildDeployActionResult {
  stage: "done" | "error";
  summary: string | null;
  log: string[];
  errorMsg: string | null;
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
