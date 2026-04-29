import type { ModPlan, PlanReviewPayload } from "../types/workflow.ts";
import { requestJson } from "./http.ts";

export interface CreateProjectRequest {
  name: string;
  target_dir: string;
}

export interface CreateProjectResponse {
  project_path: string;
}

export interface ImportProjectPackageRequest {
  package_base64: string;
  file_name?: string;
  target_dir: string;
  project_name?: string;
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

export interface ReviewModPlanRequest {
  plan: ModPlan;
  strictness?: "efficient" | "balanced" | "strict";
  bundle_decisions?: Record<string, "unresolved" | "accepted" | "split_requested" | "needs_item_revision">;
}

function assertNoBusinessError<T>(value: T | { error: string }): asserts value is T {
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
    backend: "workstation",
    method: "POST",
    body: { requirements },
  });
  assertNoBusinessError(result);
  return result;
}

export async function reviewModPlan(request: ReviewModPlanRequest): Promise<PlanReviewPayload> {
  const result = await requestJson<PlanReviewPayload | { error: string }>("/api/plan/review", {
    backend: "workstation",
    method: "POST",
    body: request,
  });
  assertNoBusinessError(result);
  return result;
}

export function createProject(request: CreateProjectRequest): Promise<CreateProjectResponse> {
  return requestJson<CreateProjectResponse>("/api/project/create", {
    backend: "workstation",
    method: "POST",
    body: request,
  });
}

export async function importProjectPackage(request: ImportProjectPackageRequest): Promise<CreateProjectResponse> {
  const result = await requestJson<CreateProjectResponse | { error: string }>("/api/project/import-package", {
    backend: "workstation",
    method: "POST",
    body: request,
  });
  assertNoBusinessError(result);
  return result;
}

export function buildProject(request: BuildProjectRequest): Promise<BuildProjectResponse> {
  return requestJson<BuildProjectResponse>("/api/project/build", {
    backend: "workstation",
    method: "POST",
    body: request,
  });
}

export function packageProject(request: PackageProjectRequest): Promise<PackageProjectResponse> {
  return requestJson<PackageProjectResponse>("/api/project/package", {
    backend: "workstation",
    method: "POST",
    body: request,
  });
}
