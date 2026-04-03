import { buildApiPath, requestJson } from "./http.ts";

export interface PlatformJobCreateItem {
  item_type: string;
  input_summary: string;
  input_payload: Record<string, unknown>;
}

export interface PlatformJobCreateRequest {
  job_type: string;
  workflow_version: string;
  input_summary: string;
  created_from: string;
  items: PlatformJobCreateItem[];
}

export interface PlatformJobStartRequest {
  triggered_by: string;
}

export interface PlatformJobCancelRequest {
  reason: string;
}

export interface PlatformJobSummary {
  id: number;
  job_type: string;
  status: string;
  workflow_version?: string;
  input_summary?: string;
  result_summary?: string;
  total_item_count?: number;
  succeeded_item_count?: number;
  failed_item_count?: number;
}

export interface PlatformJobActionResponse {
  id?: number;
  status?: string;
  ok?: boolean;
}

export interface PlatformJobItemSummary {
  id: number;
  item_index: number;
  item_type: string;
  status: string;
  result_summary: string;
  error_summary: string;
}

export interface PlatformArtifactSummary {
  id: number;
  artifact_type: string;
  file_name: string;
  result_summary: string;
}

export interface PlatformJobDetail extends PlatformJobSummary {
  error_summary?: string;
  items?: PlatformJobItemSummary[];
  artifacts?: PlatformArtifactSummary[];
}

export interface PlatformJobEventQuery {
  afterId?: number;
  limit?: number;
}

export interface PlatformJobEventView {
  event_id: number;
  event_type: string;
  job_id: number;
  occurred_at: string;
  payload: Record<string, unknown>;
  job_item_id: number | null;
  ai_execution_id: number | null;
}

export interface PlatformQuotaView {
  daily_limit: number;
  daily_used: number;
  weekly_limit: number;
  weekly_used: number;
  refunded: number;
  next_reset_at: string;
}

export function createPlatformJob(
  userId: number,
  body: PlatformJobCreateRequest,
): Promise<PlatformJobSummary> {
  return requestJson<PlatformJobSummary>(buildApiPath("/api/platform/jobs", { user_id: userId }), {
    backend: "web",
    method: "POST",
    body,
  });
}

export function startPlatformJob(
  userId: number,
  jobId: number,
  body: PlatformJobStartRequest,
): Promise<PlatformJobActionResponse> {
  return requestJson<PlatformJobActionResponse>(
    buildApiPath(`/api/platform/jobs/${jobId}/start`, { user_id: userId }),
    { backend: "web", method: "POST", body },
  );
}

export function cancelPlatformJob(
  userId: number,
  jobId: number,
  body: PlatformJobCancelRequest,
): Promise<PlatformJobActionResponse> {
  return requestJson<PlatformJobActionResponse>(
    buildApiPath(`/api/platform/jobs/${jobId}/cancel`, { user_id: userId }),
    { backend: "web", method: "POST", body },
  );
}

export function listPlatformJobs(userId: number): Promise<PlatformJobSummary[]> {
  return requestJson<PlatformJobSummary[]>(buildApiPath("/api/platform/jobs", { user_id: userId }), {
    backend: "web",
  });
}

export function getPlatformJob(userId: number, jobId: number): Promise<PlatformJobDetail> {
  return requestJson<PlatformJobDetail>(buildApiPath(`/api/platform/jobs/${jobId}`, { user_id: userId }), {
    backend: "web",
  });
}

export function listPlatformJobItems(userId: number, jobId: number): Promise<PlatformJobItemSummary[]> {
  return requestJson<PlatformJobItemSummary[]>(
    buildApiPath(`/api/platform/jobs/${jobId}/items`, { user_id: userId }),
    { backend: "web" },
  );
}

export function listPlatformJobEvents(
  userId: number,
  jobId: number,
  query: PlatformJobEventQuery = {},
): Promise<PlatformJobEventView[]> {
  return requestJson<PlatformJobEventView[]>(
    buildApiPath(`/api/platform/jobs/${jobId}/events`, {
      user_id: userId,
      after_id: query.afterId,
      limit: query.limit,
    }),
    { backend: "web" },
  );
}

export function getPlatformQuota(userId: number): Promise<PlatformQuotaView> {
  return requestJson<PlatformQuotaView>(buildApiPath("/api/platform/quota", { user_id: userId }), {
    backend: "web",
  });
}
