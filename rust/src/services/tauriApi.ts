// Tauri 端 API 实现 —— 签名权威，webApi.ts 必须实现同名同签名的所有导出。

import { invoke } from "@tauri-apps/api/core";

// -------- Health --------

export type Role = "web" | "workstation";

export interface ConfigStatus {
  path: string | null;
  filePresent: boolean;
  loaded: boolean;
  errors: string[];
}

export interface HealthReport {
  status: "ok" | "degraded";
  role: Role;
  coreVersion: string;
  serverTime: string;
  config: ConfigStatus;
}

export function getHealth(): Promise<HealthReport> {
  return invoke<HealthReport>("get_health");
}

// -------- Knowledge --------

export type OverallState = "fresh" | "missing" | "stale";
export type SourceMode = "runtime_decompiled" | "missing";

export interface GameStatus {
  sourceMode: SourceMode;
  knowledgePath: string;
  hasDecompiledSources: boolean;
}

export interface BaselibStatus {
  sourceMode: SourceMode;
  knowledgePath: string;
  hasDecompiledSources: boolean;
}

export interface KnowledgeStatus {
  overall: OverallState;
  knowledgeRoot: string;
  warnings: string[];
  game: GameStatus;
  baselib: BaselibStatus;
  embeddedTemplates: string[];
}

export function getKnowledgeStatus(): Promise<KnowledgeStatus> {
  return invoke<KnowledgeStatus>("get_knowledge_status");
}

export function checkKnowledgeStatus(): Promise<KnowledgeStatus> {
  return invoke<KnowledgeStatus>("check_knowledge_status");
}

// -------- Planning --------

export type AssetItemType =
  | "card"
  | "card_fullscreen"
  | "relic"
  | "power"
  | "character"
  | "custom_code";

export type ReviewStrictness = "efficient" | "balanced" | "strict";

export type PlanItemReviewStatus = "clear" | "needs_user_input" | "invalid";

// Mirror of ats-core::planning::PlanItem. All fields optional on input — the
// Rust side fills defaults — but the response always carries every field.
export interface PlanItem {
  id: string;
  type: AssetItemType;
  name: string;
  name_zhs?: string;
  description?: string;
  goal?: string;
  detailed_description?: string;
  implementation_notes?: string;
  needs_image?: boolean;
  image_description?: string;
  depends_on_item_ids?: string[];
  scope_boundary?: string;
  relationship_reason?: string;
  acceptance_notes?: string;
  affected_targets?: string[];
  relationship_type?: string;
  clarification_status?: string;
  clarification_questions?: string[];
  provided_image_b64?: string;
}

export interface ModPlan {
  mod_name: string;
  summary: string;
  items: PlanItem[];
}

export interface PlanValidationIssue {
  code: string;
  message: string;
  field: string;
}

export interface PlanItemValidation {
  itemId: string;
  status: PlanItemReviewStatus;
  issues: PlanValidationIssue[];
  missingFields: string[];
  clarificationQuestions: string[];
}

export interface PlanValidationResult {
  strictness: ReviewStrictness;
  items: PlanItemValidation[];
}

export function validatePlan(
  plan: ModPlan,
  strictness: ReviewStrictness = "balanced",
): Promise<PlanValidationResult> {
  return invoke<PlanValidationResult>("validate_plan_cmd", { plan, strictness });
}

export type BundleReviewStatus = "clear" | "needs_confirmation" | "split_recommended";
export type BundleDecision =
  | "unresolved"
  | "accepted"
  | "split_requested"
  | "needs_item_revision";

export interface DependencyGroup {
  itemIds: string[];
}

export interface RiskDetail {
  code: string;
  title: string;
  summary: string;
  recommendation: string;
  impact?: string;
}

export interface RecommendedAction {
  action: string;
  label: string;
  description: string;
  emphasis: string;
}

export interface ExecutionBundle {
  bundleId: string;
  itemIds: string[];
  status: BundleReviewStatus;
  reason: string;
  riskCodes: string[];
  riskDetails: RiskDetail[];
  recommendedActions: RecommendedAction[];
  blockingReason: string;
}

export interface ExecutionPlanPreview {
  strictness: ReviewStrictness;
  dependencyGroups: DependencyGroup[];
  executionBundles: ExecutionBundle[];
}

export function buildExecutionPlan(
  plan: ModPlan,
  strictness: ReviewStrictness = "balanced",
  bundleDecisions: Record<string, BundleDecision> = {},
): Promise<ExecutionPlanPreview> {
  return invoke<ExecutionPlanPreview>("build_execution_plan_cmd", {
    plan,
    strictness,
    bundleDecisions,
  });
}

// -------- Codegen --------

export interface AssetCodegenRequest {
  design_description: string;
  asset_type: string;
  asset_name: string;
  image_paths: string[];
  project_root: string;
  name_zhs: string;
  skip_build: boolean;
}

export interface CustomCodegenRequest {
  description: string;
  implementation_notes: string;
  name: string;
  project_root: string;
  skip_build: boolean;
}

export interface AssetGroupItem {
  item: PlanItem;
  image_paths: string[];
}

export interface AssetGroupRequest {
  assets: AssetGroupItem[];
  project_root: string;
}

export interface ModProjectRequest {
  project_name: string;
  target_dir: string;
}

export function codegenAssetPrompt(request: AssetCodegenRequest): Promise<string> {
  return invoke<string>("codegen_asset_prompt", { request });
}

export function codegenCustomCodePrompt(
  request: CustomCodegenRequest,
): Promise<string> {
  return invoke<string>("codegen_custom_code_prompt", { request });
}

export function codegenAssetGroupPrompt(
  request: AssetGroupRequest,
): Promise<string> {
  return invoke<string>("codegen_asset_group_prompt", { request });
}

export function codegenBuildPrompt(maxAttempts = 3): Promise<string> {
  return invoke<string>("codegen_build_prompt", { maxAttempts });
}

export function codegenCreateModProjectPrompt(
  request: ModProjectRequest,
): Promise<string> {
  return invoke<string>("codegen_create_mod_project_prompt", { request });
}

export function codegenPackagePrompt(): Promise<string> {
  return invoke<string>("codegen_package_prompt");
}
