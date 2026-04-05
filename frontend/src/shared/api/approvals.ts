import { requestJson } from "./http.ts";

export type ApprovalRiskLevel = "low" | "medium" | "high";

export interface ApprovalRequest {
  action_id: string;
  kind: string;
  title: string;
  reason: string;
  risk_level: ApprovalRiskLevel;
  requires_approval: boolean;
  status: string;
  payload: Record<string, unknown>;
  source_backend?: string;
  source_workflow?: string;
  created_at?: string;
  result?: unknown;
  error?: unknown;
}

export function summarizeApprovalPending(summary: string, requests: ApprovalRequest[]): string {
  const trimmed = summary.trim();
  return trimmed || `有 ${requests.length} 个动作等待审批`;
}

export function describeApprovalPayload(request: ApprovalRequest): string {
  const command = request.payload.command;
  if (Array.isArray(command)) {
    return command.join(" ");
  }

  const path = request.payload.path;
  if (typeof path === "string" && path) {
    return path;
  }

  return "";
}

export function listApprovals(): Promise<ApprovalRequest[]> {
  return requestJson<ApprovalRequest[]>("/api/approvals", {
    backend: "workstation",
  });
}

export function getApproval(actionId: string): Promise<ApprovalRequest> {
  return requestJson<ApprovalRequest>(`/api/approvals/${actionId}`, {
    backend: "workstation",
  });
}

export function approveApproval(actionId: string): Promise<ApprovalRequest> {
  return requestJson<ApprovalRequest>(`/api/approvals/${actionId}/approve`, {
    backend: "workstation",
    method: "POST",
  });
}

export function rejectApproval(actionId: string, reason = "Rejected from UI"): Promise<ApprovalRequest> {
  return requestJson<ApprovalRequest>(`/api/approvals/${actionId}/reject`, {
    backend: "workstation",
    method: "POST",
    body: { reason },
  });
}

export function executeApproval(actionId: string): Promise<ApprovalRequest> {
  return requestJson<ApprovalRequest>(`/api/approvals/${actionId}/execute`, {
    backend: "workstation",
    method: "POST",
  });
}
