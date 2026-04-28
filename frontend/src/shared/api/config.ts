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

export interface ImageGenerationTestResult {
  ok: boolean;
  size?: [number, number];
}

export interface PlatformQueueWorkerLeaderEvent {
  event_type: string;
  occurred_at: string;
  owner_id: string;
  leader_epoch?: number | null;
  detail?: string;
}

export interface PlatformQueueWorkerLeaderLease {
  leader_epoch: number | null;
  owner_id: string;
  owner_scope: string;
  claimed_at: string;
  renewed_at: string;
  expires_at: string;
}

export interface PlatformQueueWorkerStatus {
  available: boolean;
  reason?: string;
  owner_id?: string;
  owner_scope?: string;
  is_leader?: boolean;
  leader_epoch?: number | null;
  failover_window_seconds?: number | null;
  leader_retry_grace_seconds?: number | null;
  next_leader_retry_not_before?: string;
  last_tick_at?: string;
  last_tick_reason?: string;
  last_leader_acquired_at?: string;
  last_leader_lost_at?: string;
  current_leader?: PlatformQueueWorkerLeaderLease | null;
  recent_leader_events?: PlatformQueueWorkerLeaderEvent[];
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

export async function getLatestDetectAppPathsTask(): Promise<DetectPathsTaskResult | null> {
  return requestJson<DetectPathsTaskResult | null>("/api/config/detect_paths/latest", {
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

export async function testImageGenerationConfig(): Promise<ImageGenerationTestResult> {
  return requestJson<ImageGenerationTestResult>("/api/config/test_imggen", {
    backend: "workstation",
  });
}

export async function loadPlatformQueueWorkerStatus(): Promise<PlatformQueueWorkerStatus> {
  return requestJson<PlatformQueueWorkerStatus>("/api/platform/queue-worker-status", {
    backend: "web",
  });
}
