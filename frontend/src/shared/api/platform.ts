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
  selected_execution_profile_id?: number;
  selected_agent_backend?: string;
  selected_model?: string;
}

export interface PlatformJobStartRequest {
  triggered_by: string;
}

export interface PlatformJobSummary {
  id: number;
  job_type: string;
  status: string;
  delivery_state?: string;
  workflow_version?: string;
  selected_execution_profile_id?: number | null;
  selected_agent_backend?: string;
  selected_model?: string;
  input_summary?: string;
  result_summary?: string;
  total_item_count?: number;
  succeeded_item_count?: number;
  failed_item_count?: number;
  original_deducted?: number;
  refunded_amount?: number;
  net_consumed?: number;
  refund_reason_summary?: string;
  deferred_reason_code?: string;
  deferred_reason_message?: string;
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
  delivery_state?: string;
  result_summary: string;
  error_summary: string;
}

export interface PlatformJobEventSummary {
  event_id: number;
  event_type: string;
  job_id: number;
  occurred_at: string;
  payload: Record<string, unknown>;
  job_item_id?: number;
}

export interface PlatformArtifactSummary {
  id: number;
  artifact_type: string;
  storage_provider: string;
  object_key: string;
  file_name: string;
  mime_type?: string;
  size_bytes?: number;
  result_summary: string;
}

export interface PlatformJobDetail extends PlatformJobSummary {
  error_summary?: string;
  items?: PlatformJobItemSummary[];
  artifacts?: PlatformArtifactSummary[];
}

export interface PlatformQuotaView {
  total_limit: number;
  used_amount: number;
  refunded_amount: number;
  adjusted_amount: number;
  remaining: number;
  status: string;
}

export interface PlatformExecutionProfile {
  id: number;
  display_name: string;
  agent_backend: string;
  model: string;
  description: string;
  recommended: boolean;
  available: boolean;
}

export interface PlatformExecutionProfileListView {
  items: PlatformExecutionProfile[];
}
