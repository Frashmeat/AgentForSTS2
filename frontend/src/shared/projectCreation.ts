import { createProject } from "./api/index.ts";
import type { CreateProjectRequest, CreateProjectResponse } from "./api/workflow.ts";

const PROJECT_ROOT_EXAMPLE = "请填写完整的项目路径，例如 E:/STS2mod/MyMod";

export function deriveCreateProjectRequest(projectRoot: string): CreateProjectRequest {
  const normalized = projectRoot.trim().replace(/[\\/]+$/u, "");
  const match = normalized.match(/^(.*[\\/])([^\\/]+)$/u);

  if (!match) {
    throw new Error(PROJECT_ROOT_EXAMPLE);
  }

  const projectName = match[2]?.trim();
  let targetDir = match[1]?.replace(/[\\/]+$/u, "").trim();

  if (!projectName || !targetDir) {
    throw new Error(PROJECT_ROOT_EXAMPLE);
  }

  if (/^[A-Za-z]:$/u.test(targetDir)) {
    targetDir = `${targetDir}/`;
  }

  return {
    name: projectName,
    target_dir: targetDir,
  };
}

export type CreateProjectRequester = (request: CreateProjectRequest) => Promise<CreateProjectResponse>;

export async function createProjectFromRoot(
  projectRoot: string,
  requestProject: CreateProjectRequester = createProject,
): Promise<CreateProjectResponse> {
  return requestProject(deriveCreateProjectRequest(projectRoot));
}
