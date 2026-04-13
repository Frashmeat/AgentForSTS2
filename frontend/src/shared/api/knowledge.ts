import { requestJson } from "./http.ts";

export type KnowledgeStatusKind = "checking" | "fresh" | "stale" | "missing" | "refreshing" | "error";
export type KnowledgeRefreshTaskStatus = "pending" | "running" | "completed" | "failed";

export interface KnowledgeStatus {
  status: KnowledgeStatusKind;
  generated_at?: string | null;
  checked_at?: string | null;
  warnings: string[];
  game: {
    configured_path?: string;
    version?: string | null;
    current_version?: string | null;
    matches?: boolean | null;
    version_source?: string | null;
    source_mode?: "runtime_decompiled" | "missing";
    knowledge_path?: string;
    decompiled_src_path?: string;
  };
  baselib: {
    release_tag?: string | null;
    latest_release_tag?: string | null;
    matches?: boolean | null;
    release_url?: string;
    source_mode?: "runtime_decompiled" | "missing";
    knowledge_path?: string;
    decompiled_src_path?: string;
  };
}

export interface KnowledgeRefreshTask {
  task_id: string;
  status: KnowledgeRefreshTaskStatus;
  current_step: string;
  notes: string[];
  error?: string | null;
  can_cancel: boolean;
}

export async function loadKnowledgeStatus(): Promise<KnowledgeStatus> {
  return requestJson<KnowledgeStatus>("/api/knowledge/status", {
    backend: "workstation",
  });
}

export async function checkKnowledgeStatus(): Promise<KnowledgeStatus> {
  return requestJson<KnowledgeStatus>("/api/knowledge/check", {
    backend: "workstation",
    method: "POST",
  });
}

export async function startRefreshKnowledgeTask(): Promise<KnowledgeRefreshTask> {
  return requestJson<KnowledgeRefreshTask>("/api/knowledge/refresh/start", {
    backend: "workstation",
    method: "POST",
  });
}

export async function getRefreshKnowledgeTask(taskId: string): Promise<KnowledgeRefreshTask> {
  return requestJson<KnowledgeRefreshTask>(`/api/knowledge/refresh/${encodeURIComponent(taskId)}`, {
    backend: "workstation",
  });
}
