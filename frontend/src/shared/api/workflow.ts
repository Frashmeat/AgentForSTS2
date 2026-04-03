import type { ModPlan } from "../types/workflow.ts";
import { requestJson } from "./http.ts";

export interface CreateProjectRequest {
  name: string;
  target_dir: string;
}

export interface CreateProjectResponse {
  project_path: string;
}

export interface BuildProjectRequest {
  project_root: string;
}

export interface BuildProjectResponse {
  success: boolean;
  output: string;
}

export interface PackageProjectRequest {
  project_root: string;
}

export interface PackageProjectResponse {
  success: boolean;
}

function assertNoBusinessError(value: unknown): void {
  if (
    typeof value === "object" &&
    value !== null &&
    "error" in value &&
    typeof (value as { error?: unknown }).error === "string"
  ) {
    throw new Error((value as { error: string }).error);
  }
}

export async function generateModPlan(requirements: string): Promise<ModPlan> {
  const result = await requestJson<ModPlan | { error: string }>("/api/plan", {
    method: "POST",
    body: { requirements },
  });
  assertNoBusinessError(result);
  return result;
}

export function createProject(request: CreateProjectRequest): Promise<CreateProjectResponse> {
  return requestJson<CreateProjectResponse>("/api/project/create", {
    method: "POST",
    body: request,
  });
}

export function buildProject(request: BuildProjectRequest): Promise<BuildProjectResponse> {
  return requestJson<BuildProjectResponse>("/api/project/build", {
    method: "POST",
    body: request,
  });
}

export function packageProject(request: PackageProjectRequest): Promise<PackageProjectResponse> {
  return requestJson<PackageProjectResponse>("/api/project/package", {
    method: "POST",
    body: request,
  });
}
