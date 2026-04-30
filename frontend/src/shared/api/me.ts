import { buildApiPath, buildBackendUrl, requestJson } from "./http.ts";
import type {
  PlatformExecutionProfileListView,
  PlatformJobDetail,
  PlatformJobActionResponse,
  PlatformJobCreateRequest,
  PlatformJobSummary,
  PlatformJobItemSummary,
  PlatformJobEventSummary,
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

export interface MyServerPreferenceView {
  default_execution_profile_id: number | null;
  display_name: string;
  agent_backend: string;
  model: string;
  available: boolean;
  updated_at: string | null;
}

export interface UploadedAssetView {
  uploaded_asset_ref: string;
  file_name: string;
  mime_type: string;
  size_bytes: number;
  created_at: string;
}

export interface ServerWorkspaceView {
  server_project_ref: string;
  project_name: string;
  workspace_root: string;
  created_at: string;
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

export function listPlatformExecutionProfiles(): Promise<PlatformExecutionProfileListView> {
  return requestJson<PlatformExecutionProfileListView>(buildApiPath("/api/platform/execution-profiles", {}), {
    backend: "web",
  });
}

export function getMyServerPreferences(): Promise<MyServerPreferenceView> {
  return requestJson<MyServerPreferenceView>(buildApiPath("/api/me/server-preferences", {}), {
    backend: "web",
  });
}

export function updateMyServerPreferences(body: {
  default_execution_profile_id: number | null;
}): Promise<MyServerPreferenceView> {
  return requestJson<MyServerPreferenceView>(buildApiPath("/api/me/server-preferences", {}), {
    backend: "web",
    method: "PUT",
    body,
  });
}

export function uploadMyServerAsset(body: {
  file_name: string;
  content_base64: string;
  mime_type?: string;
}): Promise<UploadedAssetView> {
  return requestJson<UploadedAssetView>(buildApiPath("/api/me/upload-assets", {}), {
    backend: "web",
    method: "POST",
    body,
  });
}

export function createMyServerWorkspace(body: { project_name: string }): Promise<ServerWorkspaceView> {
  return requestJson<ServerWorkspaceView>(buildApiPath("/api/me/server-workspaces", {}), {
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

export function listMyJobEvents(jobId: number): Promise<PlatformJobEventSummary[]> {
  return requestJson<PlatformJobEventSummary[]>(buildApiPath(`/api/me/jobs/${jobId}/events`, {}), {
    backend: "web",
  });
}

export function getMyArtifactDownloadUrl(artifactId: number): string {
  return buildBackendUrl(buildApiPath(`/api/me/artifacts/${artifactId}/download`, {}), "web");
}
