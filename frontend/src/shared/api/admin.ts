import { requestJson } from "./http.ts";

function withQuery(path: string, query: Record<string, string | number | undefined>): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (typeof value === "undefined") {
      continue;
    }
    params.set(key, String(value));
  }
  const queryString = params.toString();
  return queryString ? `${path}?${queryString}` : path;
}

export function listAdminJobExecutions(jobId: number): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(`/api/admin/jobs/${jobId}/executions`);
}

export function getAdminExecution(executionId: number): Promise<Record<string, unknown>> {
  return requestJson<Record<string, unknown>>(`/api/admin/executions/${executionId}`);
}

export function listAdminQuotaRefunds(userId?: number): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(withQuery("/api/admin/quota/refunds", { user_id: userId }));
}

export function listAdminAuditEvents(jobId?: number): Promise<Array<Record<string, unknown>>> {
  return requestJson<Array<Record<string, unknown>>>(withQuery("/api/admin/audit/events", { job_id: jobId }));
}
