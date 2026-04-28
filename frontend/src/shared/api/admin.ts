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
  user_id?: number;
  ai_execution_id: number;
  charge_status: string;
  refund_reason: string;
  quota_amount?: number;
  created_at?: string;
}

export interface AdminUserQuotaView {
  total_limit: number;
  used_amount: number;
  refunded_amount: number;
  adjusted_amount: number;
  remaining: number;
  status: string;
}

export interface AdminUserListItem {
  user_id: number;
  username: string;
  email: string;
  email_verified: boolean;
  is_admin: boolean;
  created_at: string;
  quota: AdminUserQuotaView;
  anomaly_flags: string[];
}

export interface AdminUserListView {
  items: AdminUserListItem[];
  total: number;
  limit: number;
  offset: number;
}

export interface AdminUserDetail extends AdminUserListItem {
  email_verified_at?: string | null;
}

export interface AdminQuotaLedgerItem {
  ledger_id: number;
  ledger_type: string;
  amount: number;
  balance_after: number;
  reason_code: string;
  reason: string;
  ai_execution_id?: number | null;
  created_at: string;
}

export interface AdminQuotaLedgerListView {
  items: AdminQuotaLedgerItem[];
}

export interface AdjustAdminUserQuotaRequest {
  direction: "grant" | "deduct";
  amount: number;
  reason: string;
}

export interface AdminExecutionProfileListItem {
  id: number;
  code: string;
  display_name: string;
  agent_backend: string;
  model: string;
  enabled: boolean;
  recommended: boolean;
  sort_order: number;
}

export interface AdminExecutionProfileListView {
  items: AdminExecutionProfileListItem[];
}

export interface AdminServerCredentialListItem {
  id: number;
  execution_profile_id: number;
  provider: string;
  auth_type: string;
  label: string;
  base_url: string;
  priority: number;
  enabled: boolean;
  health_status: string;
  last_checked_at?: string | null;
  last_error_code: string;
  last_error_message: string;
}

export interface AdminServerCredentialListView {
  items: AdminServerCredentialListItem[];
}

export interface CreateAdminServerCredentialRequest {
  execution_profile_id: number;
  provider: string;
  auth_type: string;
  credential: string;
  secret?: string;
  base_url?: string;
  label?: string;
  priority?: number;
  enabled?: boolean;
}

export interface UpdateAdminServerCredentialRequest {
  execution_profile_id: number;
  provider: string;
  auth_type: string;
  credential?: string;
  secret?: string;
  base_url?: string;
  label?: string;
  priority?: number;
  enabled?: boolean;
}

export interface AdminServerCredentialHealthCheckView {
  credential_id: number;
  health_status: string;
  error_code: string;
  error_message: string;
  checked_at?: string | null;
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

export function listAdminUsers(params: {
  query?: string;
  email_verified?: boolean;
  is_admin?: boolean;
  quota_status?: string;
  anomaly?: string;
  limit?: number;
  offset?: number;
} = {}): Promise<AdminUserListView> {
  return requestJson<AdminUserListView>(
    buildApiPath("/api/admin/users", {
      query: params.query,
      email_verified: typeof params.email_verified === "boolean" ? String(params.email_verified) : undefined,
      is_admin: typeof params.is_admin === "boolean" ? String(params.is_admin) : undefined,
      quota_status: params.quota_status,
      anomaly: params.anomaly,
      limit: params.limit,
      offset: params.offset,
    }),
    { backend: "web" },
  );
}

export function getAdminUser(userId: number): Promise<AdminUserDetail> {
  return requestJson<AdminUserDetail>(`/api/admin/users/${userId}`, {
    backend: "web",
  });
}

export function getAdminUserQuota(userId: number): Promise<AdminUserQuotaView> {
  return requestJson<AdminUserQuotaView>(`/api/admin/users/${userId}/quota`, {
    backend: "web",
  });
}

export function listAdminUserQuotaLedger(userId: number, afterId?: number, limit?: number): Promise<AdminQuotaLedgerListView> {
  return requestJson<AdminQuotaLedgerListView>(
    buildApiPath(`/api/admin/users/${userId}/quota/ledger`, { after_id: afterId, limit }),
    { backend: "web" },
  );
}

export function adjustAdminUserQuota(userId: number, body: AdjustAdminUserQuotaRequest): Promise<AdminUserQuotaView> {
  return requestJson<AdminUserQuotaView>(`/api/admin/users/${userId}/quota/adjust`, {
    backend: "web",
    method: "POST",
    body,
  });
}

export function listAdminExecutionProfiles(): Promise<AdminExecutionProfileListView> {
  return requestJson<AdminExecutionProfileListView>("/api/admin/platform/execution-profiles", {
    backend: "web",
  });
}

export function listAdminServerCredentials(executionProfileId?: number): Promise<AdminServerCredentialListView> {
  return requestJson<AdminServerCredentialListView>(
    buildApiPath("/api/admin/platform/server-credentials", { execution_profile_id: executionProfileId }),
    {
      backend: "web",
    },
  );
}

export function createAdminServerCredential(
  body: CreateAdminServerCredentialRequest,
): Promise<AdminServerCredentialListItem> {
  return requestJson<AdminServerCredentialListItem>("/api/admin/platform/server-credentials", {
    backend: "web",
    method: "POST",
    body,
  });
}

export function updateAdminServerCredential(
  credentialId: number,
  body: UpdateAdminServerCredentialRequest,
): Promise<AdminServerCredentialListItem> {
  return requestJson<AdminServerCredentialListItem>(`/api/admin/platform/server-credentials/${credentialId}`, {
    backend: "web",
    method: "PUT",
    body,
  });
}

export function enableAdminServerCredential(credentialId: number): Promise<AdminServerCredentialListItem> {
  return requestJson<AdminServerCredentialListItem>(`/api/admin/platform/server-credentials/${credentialId}/enable`, {
    backend: "web",
    method: "POST",
  });
}

export function disableAdminServerCredential(credentialId: number): Promise<AdminServerCredentialListItem> {
  return requestJson<AdminServerCredentialListItem>(`/api/admin/platform/server-credentials/${credentialId}/disable`, {
    backend: "web",
    method: "POST",
  });
}

export function runAdminServerCredentialHealthCheck(
  credentialId: number,
): Promise<AdminServerCredentialHealthCheckView> {
  return requestJson<AdminServerCredentialHealthCheckView>(
    `/api/admin/platform/server-credentials/${credentialId}/health-check`,
    {
      backend: "web",
      method: "POST",
    },
  );
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
