import { buildApiPath, requestJson } from "./http.ts";

export interface AdminExecutionListItem {
  id: number;
  job_id: number;
  job_item_id: number;
  status: string;
  provider: string;
  model: string;
}

export interface AdminExecutionDetail extends AdminExecutionListItem {
  request_idempotency_key: string;
  input_summary: string;
  result_summary: string;
  error_summary: string;
  step_protocol_version: string;
  result_schema_version: string;
}

export interface AdminQuotaRefundItem {
  ai_execution_id: number;
  charge_status: string;
  refund_reason: string;
}

export interface AdminAuditEvent {
  event_id: number;
  event_type: string;
  job_id: number;
  job_item_id?: number | null;
  ai_execution_id?: number | null;
  occurred_at: string;
  payload: Record<string, unknown>;
}

export function listAdminJobExecutions(jobId: number): Promise<AdminExecutionListItem[]> {
  return requestJson<AdminExecutionListItem[]>(`/api/admin/jobs/${jobId}/executions`, {
    backend: "web",
  });
}

export function getAdminExecution(executionId: number): Promise<AdminExecutionDetail> {
  return requestJson<AdminExecutionDetail>(`/api/admin/executions/${executionId}`, {
    backend: "web",
  });
}

export function listAdminQuotaRefunds(userId?: number): Promise<AdminQuotaRefundItem[]> {
  return requestJson<AdminQuotaRefundItem[]>(buildApiPath("/api/admin/quota/refunds", { user_id: userId }), {
    backend: "web",
  });
}

export function listAdminAuditEvents(
  jobId?: number,
  eventTypePrefix?: string,
  afterId?: number,
  limit?: number,
): Promise<AdminAuditEvent[]> {
  return requestJson<AdminAuditEvent[]>(
    buildApiPath("/api/admin/audit/events", {
      job_id: jobId,
      event_type_prefix: eventTypePrefix,
      after_id: afterId,
      limit,
    }),
    {
      backend: "web",
    },
  );
}
