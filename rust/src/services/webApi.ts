// Web 端 API 实现 —— 必须实现 tauriApi.ts 全部导出。

import type {
  AssetCodegenRequest,
  AssetGroupRequest,
  BundleDecision,
  CompletionRequest,
  CompletionResponse,
  CustomCodegenRequest,
  ExecutionPlanPreview,
  HealthReport,
  KnowledgeStatus,
  ModPlan,
  ModProjectRequest,
  PlanValidationResult,
  ReviewStrictness,
} from "./tauriApi";

const API_BASE = "/api";

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`);
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

async function postJson<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, { method: "POST" });
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

export function getHealth(): Promise<HealthReport> {
  return getJson<HealthReport>("/health");
}

export function getKnowledgeStatus(): Promise<KnowledgeStatus> {
  return getJson<KnowledgeStatus>("/knowledge/status");
}

export function checkKnowledgeStatus(): Promise<KnowledgeStatus> {
  return postJson<KnowledgeStatus>("/knowledge/check");
}

export async function validatePlan(
  plan: ModPlan,
  strictness: ReviewStrictness = "balanced",
): Promise<PlanValidationResult> {
  const response = await fetch(
    `${API_BASE}/planning/validate?strictness=${encodeURIComponent(strictness)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(plan),
    },
  );
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<PlanValidationResult>;
}

export async function buildExecutionPlan(
  plan: ModPlan,
  strictness: ReviewStrictness = "balanced",
  bundleDecisions: Record<string, BundleDecision> = {},
): Promise<ExecutionPlanPreview> {
  const response = await fetch(
    `${API_BASE}/planning/execution-plan?strictness=${encodeURIComponent(strictness)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ plan, bundle_decisions: bundleDecisions }),
    },
  );
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<ExecutionPlanPreview>;
}

async function codegenPost(path: string, body: unknown): Promise<string> {
  const response = await fetch(`${API_BASE}/codegen/${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  const payload = (await response.json()) as { prompt: string };
  return payload.prompt;
}

export function codegenAssetPrompt(request: AssetCodegenRequest): Promise<string> {
  return codegenPost("asset-prompt", request);
}

export function codegenCustomCodePrompt(
  request: CustomCodegenRequest,
): Promise<string> {
  return codegenPost("custom-code-prompt", request);
}

export function codegenAssetGroupPrompt(
  request: AssetGroupRequest,
): Promise<string> {
  return codegenPost("asset-group-prompt", request);
}

export function codegenBuildPrompt(maxAttempts = 3): Promise<string> {
  return codegenPost("build-prompt", { max_attempts: maxAttempts });
}

export function codegenCreateModProjectPrompt(
  request: ModProjectRequest,
): Promise<string> {
  return codegenPost("create-mod-project-prompt", request);
}

export function codegenPackagePrompt(): Promise<string> {
  return codegenPost("package-prompt", {});
}

export async function llmComplete(
  request: CompletionRequest,
): Promise<CompletionResponse> {
  const response = await fetch(`${API_BASE}/llm/complete`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });
  if (!response.ok) {
    throw new Error(`${response.status} ${await response.text()}`);
  }
  return response.json() as Promise<CompletionResponse>;
}

/**
 * Web 端流式 stub —— SSE 集成待 stage 4.2。当前直接走非流式 fallback。
 * 当 LlmCard 仅调 llmComplete 时不会触发此分支。
 */
export async function llmStartStream(
  _requestId: string,
  _request: CompletionRequest,
): Promise<void> {
  throw new Error("Web streaming not yet wired (use llmComplete for now)");
}
