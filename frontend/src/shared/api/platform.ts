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

export interface PlatformJobDetail extends PlatformJobSummary {
  error_summary?: string;
  items?: Array<Record<string, unknown>>;
  artifacts?: Array<Record<string, unknown>>;
}

export interface PlatformJobEventQuery {
  afterId?: number;
  limit?: number;
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
    method: "POST",
    body,
  });
}

export function startPlatformJob(
  userId: number,
  jobId: number,
  body: PlatformJobStartRequest,
): Promise<Record<string, unknown>> {
  return requestJson<Record<string, unknown>>(
    buildApiPath(`/api/platform/jobs/${jobId}/start`, { user_id: userId }),
    { method: "POST", body },
  );
}

export function cancelPlatformJob(
  userId: number,
  jobId: number,
  body: PlatformJobCancelRequest,
): Promise<Record<string, unknown>> {
  return requestJson<Record<string, unknown>>(
    buildApiPath(`/api/platform/jobs/${jobId}/cancel`, { user_id: userId }),
    { method: "POST", body },
  );
}

export function listPlatformJobs(userId: number): Promise<PlatformJobSummary[]> {
  return requestJson<PlatformJobSummary[]>(buildApiPath("/api/platform/jobs", { user_id: userId }));
}

export function getPlatformJob(userId: number, jobId: number): Promise<PlatformJobDetail> {
  return requestJson<PlatformJobDetail>(buildApiPath(`/api/platform/jobs/${jobId}`, { user_id: userId }));
}

export function listPlatformJobItems(userId: number, jobId: number): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(
    buildApiPath(`/api/platform/jobs/${jobId}/items`, { user_id: userId }),
  );
}

export function listPlatformJobEvents(
  userId: number,
  jobId: number,
  query: PlatformJobEventQuery = {},
): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(
    buildApiPath(`/api/platform/jobs/${jobId}/events`, {
      user_id: userId,
      after_id: query.afterId,
      limit: query.limit,
    }),
  );
}

export function getPlatformQuota(userId: number): Promise<PlatformQuotaView> {
  return requestJson<PlatformQuotaView>(buildApiPath("/api/platform/quota", { user_id: userId }));
}
