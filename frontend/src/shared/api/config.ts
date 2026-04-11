import { requestJson } from "./http.ts";

export interface AppConfig {
  default_project_root?: string;
  sts2_path?: string;
  godot_exe_path?: string;
  [key: string]: unknown;
}

export interface DetectPathsResult {
  sts2_path?: string;
  godot_exe_path?: string;
  notes: string[];
}

export type DetectPathsTaskStatus = "pending" | "running" | "completed" | "cancelled" | "failed";

export interface DetectPathsTaskResult {
  task_id: string;
  status: DetectPathsTaskStatus;
  current_step: string;
  notes: string[];
  sts2_path?: string;
  godot_exe_path?: string;
  error?: string | null;
  can_cancel: boolean;
}

export interface LocalAiCapabilityStatus {
  text_ai_available: boolean;
  code_agent_available: boolean;
  image_ai_available: boolean;
  text_ai_missing_reasons?: string[];
  code_agent_missing_reasons?: string[];
  image_ai_missing_reasons?: string[];
}

export interface PickPathRequest {
  kind: "file" | "directory";
  title?: string;
  initial_path?: string;
  filters?: string[][];
}

export interface PickPathResult {
  path?: string | null;
}

export async function loadAppConfig(): Promise<AppConfig> {
  return requestJson<AppConfig>("/api/config", {
    backend: "workstation",
  });
}

export async function updateAppConfig(patch: Partial<AppConfig>): Promise<AppConfig> {
  return requestJson<AppConfig>("/api/config", {
    backend: "workstation",
    method: "PATCH",
    body: patch,
  });
}

export async function detectAppPaths(): Promise<DetectPathsResult> {
  return requestJson<DetectPathsResult>("/api/config/detect_paths", {
    backend: "workstation",
  });
}

export async function startDetectAppPaths(): Promise<DetectPathsTaskResult> {
  return requestJson<DetectPathsTaskResult>("/api/config/detect_paths/start", {
    backend: "workstation",
    method: "POST",
  });
}

export async function getDetectAppPathsTask(taskId: string): Promise<DetectPathsTaskResult> {
  return requestJson<DetectPathsTaskResult>(`/api/config/detect_paths/${encodeURIComponent(taskId)}`, {
    backend: "workstation",
  });
}

export async function cancelDetectAppPathsTask(taskId: string): Promise<DetectPathsTaskResult> {
  return requestJson<DetectPathsTaskResult>(`/api/config/detect_paths/${encodeURIComponent(taskId)}/cancel`, {
    backend: "workstation",
    method: "POST",
  });
}

export async function pickAppPath(body: PickPathRequest): Promise<PickPathResult> {
  return requestJson<PickPathResult>("/api/config/pick_path", {
    backend: "workstation",
    method: "POST",
    body,
  });
}

export async function loadLocalAiCapabilityStatus(): Promise<LocalAiCapabilityStatus> {
  return requestJson<LocalAiCapabilityStatus>("/api/config/local_ai_capability_status", {
    backend: "workstation",
  });
}
