import type { ApprovalRequest } from "../api/approvals";

export type WorkflowScope = "project" | "text" | "image" | "agent" | "build";
export type WorkflowLogChannel = "raw" | "stderr" | "system" | "stage";

export type WorkflowErrorPayload = Record<string, unknown> & {
  message: string;
  code?: string;
  detail?: string;
  request_id?: string;
  traceback?: string;
};

export interface PlanValidationIssue {
  code: string;
  message: string;
  field?: string;
}

export interface PlanItemValidation {
  item_id: string;
  status: "clear" | "needs_user_input" | "invalid";
  issues: PlanValidationIssue[];
  missing_fields: string[];
  clarification_questions: string[];
}

export interface PlanValidationResult {
  strictness: string;
  items: PlanItemValidation[];
}

export interface DependencyGroupPreview {
  item_ids: string[];
}

export interface ExecutionBundlePreview {
  item_ids: string[];
  status: "clear" | "needs_confirmation" | "split_recommended";
  reason: string;
  risk_codes: string[];
}

export interface ExecutionPlanPreview {
  strictness: string;
  dependency_groups: DependencyGroupPreview[];
  execution_bundles: ExecutionBundlePreview[];
}

export interface PlanReviewPayload {
  strictness: string;
  validation: PlanValidationResult;
  execution_plan: ExecutionPlanPreview;
}

export type WorkflowEvent =
  | { event: "planning"; stage: "planning" }
  | { event: "plan_ready"; stage: "plan_ready"; plan: ModPlan; review?: PlanReviewPayload }
  | { event: "stage_update"; stage: string; scope: WorkflowScope; message: string; item_id?: string }
  | { event: "progress"; stage: "progress"; message: string }
  | { event: "batch_progress"; stage: "batch_progress"; message: string }
  | { event: "batch_started"; stage: "batch_started"; items: PlanItem[] }
  | { event: "item_started"; stage: "item_started"; item_id: string; name: string; type: string }
  | { event: "item_progress"; stage: "item_progress"; item_id: string; message: string }
  | { event: "prompt_preview"; stage: "prompt_preview"; prompt: string; negative_prompt: string; fallback_warning?: string }
  | { event: "image_ready"; stage: "image_ready"; image: string; index: number; prompt: string }
  | { event: "item_image_ready"; stage: "item_image_ready"; item_id: string; image: string; index: number; prompt: string }
  | { event: "agent_stream"; stage: "agent_stream"; chunk: string; source?: string; channel?: WorkflowLogChannel; model?: string }
  | { event: "item_agent_stream"; stage: "item_agent_stream"; item_id: string; chunk: string; source?: string; channel?: WorkflowLogChannel; model?: string }
  | { event: "approval_pending"; stage: "approval_pending"; summary: string; requests: ApprovalRequest[] }
  | { event: "item_approval_pending"; stage: "item_approval_pending"; item_id: string; summary: string; requests: ApprovalRequest[] }
  | { event: "done"; stage: "done"; success: boolean; image_paths?: string[]; agent_output?: string }
  | { event: "item_done"; stage: "item_done"; item_id: string; success: boolean }
  | { event: "batch_done"; stage: "batch_done"; success_count: number; error_count: number }
  | ({ event: "error"; stage: "error" } & WorkflowErrorPayload)
  | ({ event: "item_error"; stage: "item_error"; item_id: string } & WorkflowErrorPayload);

export interface PlanItem {
  id: string;
  type: string;
  name: string;
  name_zhs: string;
  description: string;
  goal: string;
  detailed_description: string;
  implementation_notes: string;
  needs_image: boolean;
  image_description: string;
  depends_on: string[];
  scope_boundary: string;
  dependency_reason: string;
  acceptance_notes: string;
  affected_targets: string[];
  coupling_kind: string;
  clarification_status: string;
  clarification_questions: string[];
  provided_image_b64?: string;
}

export interface ModPlan {
  mod_name: string;
  summary: string;
  items: PlanItem[];
}
