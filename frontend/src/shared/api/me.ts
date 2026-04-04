import { buildApiPath, requestJson } from "./http.ts";
import type {
  PlatformJobDetail,
  PlatformJobActionResponse,
  PlatformJobCreateRequest,
  PlatformJobSummary,
  PlatformJobItemSummary,
  PlatformQuotaView,
  PlatformJobStartRequest,
} from "./platform.ts";

export interface CurrentUserProfile {
  user_id: number;
  username: string;
  email: string;
  email_verified: boolean;
  created_at: string;
  email_verified_at?: string | null;
}

export function getMyProfile(): Promise<CurrentUserProfile> {
  return requestJson<CurrentUserProfile>(buildApiPath("/api/me/profile", {}), {
    backend: "web",
  });
}

export function getMyQuota(): Promise<PlatformQuotaView> {
  return requestJson<PlatformQuotaView>(buildApiPath("/api/me/quota", {}), {
    backend: "web",
  });
}

export function listMyJobs(): Promise<PlatformJobSummary[]> {
  return requestJson<PlatformJobSummary[]>(buildApiPath("/api/me/jobs", {}), {
    backend: "web",
  });
}

export function createMyJob(body: PlatformJobCreateRequest): Promise<PlatformJobSummary> {
  return requestJson<PlatformJobSummary>(buildApiPath("/api/me/jobs", {}), {
    backend: "web",
    method: "POST",
    body,
  });
}

export function getMyJob(jobId: number): Promise<PlatformJobDetail> {
  return requestJson<PlatformJobDetail>(buildApiPath(`/api/me/jobs/${jobId}`, {}), {
    backend: "web",
  });
}

export function startMyJob(jobId: number, body: PlatformJobStartRequest): Promise<PlatformJobActionResponse> {
  return requestJson<PlatformJobActionResponse>(buildApiPath(`/api/me/jobs/${jobId}/start`, {}), {
    backend: "web",
    method: "POST",
    body,
  });
}

export function listMyJobItems(jobId: number): Promise<PlatformJobItemSummary[]> {
  return requestJson<PlatformJobItemSummary[]>(buildApiPath(`/api/me/jobs/${jobId}/items`, {}), {
    backend: "web",
  });
}
