import { requestJson } from "./http.ts";

export interface WorkflowMigrationFlags {
  use_modular_single_workflow: boolean;
  use_modular_batch_workflow: boolean;
  use_unified_ws_contract: boolean;
}

export interface AppConfig {
  default_project_root?: string;
  migration?: Partial<WorkflowMigrationFlags>;
  [key: string]: unknown;
}

export interface DetectPathsResult {
  sts2_path?: string;
  godot_exe_path?: string;
  notes: string[];
}

export interface LocalAiCapabilityStatus {
  text_ai_available: boolean;
  image_ai_available: boolean;
}

const DEFAULT_MIGRATION_FLAGS: WorkflowMigrationFlags = {
  use_modular_single_workflow: false,
  use_modular_batch_workflow: false,
  use_unified_ws_contract: false,
};

export function resolveMigrationFlags(config?: Pick<AppConfig, "migration"> | null): WorkflowMigrationFlags {
  return {
    ...DEFAULT_MIGRATION_FLAGS,
    ...(config?.migration ?? {}),
  };
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

export async function loadLocalAiCapabilityStatus(): Promise<LocalAiCapabilityStatus> {
  return requestJson<LocalAiCapabilityStatus>("/api/config/local_ai_capability_status", {
    backend: "workstation",
  });
}
