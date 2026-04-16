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
  original_deducted?: number;
  refunded_amount?: number;
  net_consumed?: number;
  refund_reason_summary?: string;
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
  file_name: string;
  result_summary: string;
}

export interface PlatformJobDetail extends PlatformJobSummary {
  error_summary?: string;
  items?: PlatformJobItemSummary[];
  artifacts?: PlatformArtifactSummary[];
}

export interface PlatformQuotaView {
  daily_limit: number;
  daily_used: number;
  weekly_limit: number;
  weekly_used: number;
  refunded: number;
  next_reset_at: string;
}
