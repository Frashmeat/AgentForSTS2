import { buildApiPath, requestJson } from "./http.ts";

export function listAdminJobExecutions(jobId: number): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(`/api/admin/jobs/${jobId}/executions`);
}

export function getAdminExecution(executionId: number): Promise<Record<string, unknown>> {
  return requestJson<Record<string, unknown>>(`/api/admin/executions/${executionId}`);
}

export function listAdminQuotaRefunds(userId?: number): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(buildApiPath("/api/admin/quota/refunds", { user_id: userId }));
}

export function listAdminAuditEvents(jobId?: number): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(buildApiPath("/api/admin/audit/events", { job_id: jobId }));
}
