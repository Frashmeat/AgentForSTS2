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
  TruthSnapshotStatus,
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

export function getHealth(): Promise<HealthReport> {
  return getJson<HealthReport>("/health");
}

export function getLocalCapabilitiesSync(): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("getLocalCapabilitiesSync"));
}
export function getLocalCapabilitiesFull(): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("getLocalCapabilitiesFull"));
}

export function getTruthSnapshotStatus(): Promise<TruthSnapshotStatus> {
  return Promise.resolve().then(() => desktopOnly("getTruthSnapshotStatus"));
}

export function checkTruthSnapshotStatus(): Promise<TruthSnapshotStatus> {
  return Promise.resolve().then(() => desktopOnly("checkTruthSnapshotStatus"));
}
export function analyzeModProject(_projectRoot: string): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("analyzeModProject"));
}
export function imageProcStatus(): Promise<{ state: "idle" }> {
  // Web 模式不跑 ML prewarm，永远 Idle。
  return Promise.resolve({ state: "idle" });
}
export function getSettingsSnapshot(): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("getSettingsSnapshot"));
}
export function openConfigInEditor(): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("openConfigInEditor"));
}
export function saveSettingsPatch(_patch: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("saveSettingsPatch"));
}
export function planArtifactSave(_status: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("planArtifactSave"));
}
export function planArtifactLoad(_itemId: string): Promise<null> {
  return Promise.resolve(null);
}
export function planArtifactList(): Promise<never[]> {
  return Promise.resolve([]);
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

// -------- Project / Runs stubs（桌面专属，Web 端 stage 3.1a 后接 sqlx）--------

function desktopOnly(name: string): never {
  throw new Error(`${name} is desktop-only (Tauri); Web sqlx pending stage 3.1a`);
}

export function listRecentProjects(): Promise<never[]> {
  return Promise.resolve([]);
}
export function createProject(
  _parentDir: string,
  _name: string,
  _gameId: string,
): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("createProject"));
}
export function openProject(_path: string): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("openProject"));
}
export function closeProject(): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("closeProject"));
}
export function currentProject(): Promise<null> {
  return Promise.resolve(null);
}
export function forgetRecentProject(_path: string): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("forgetRecentProject"));
}
export function submitTextGenerateRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitTextGenerateRun"));
}
export function getRun(_id: string): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("getRun"));
}
export function listRuns(): Promise<never[]> {
  return Promise.resolve([]);
}
export function cancelRun(_id: string): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("cancelRun"));
}
export function submitCodeGenerateRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitCodeGenerateRun"));
}
export function submitBuildProjectRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitBuildProjectRun"));
}
export function submitLogAnalysisRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitLogAnalysisRun"));
}
export function submitPackageProjectRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitPackageProjectRun"));
}
export function submitBatchCustomCodeRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitBatchCustomCodeRun"));
}
export function submitSingleAssetPlanRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitSingleAssetPlanRun"));
}
export function submitTruthSnapshotRefreshRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitTruthSnapshotRefreshRun"));
}
export function submitAssetGenerateRun(_request: unknown): Promise<never> {
  return Promise.resolve().then(() => desktopOnly("submitAssetGenerateRun"));
}
export function discoverSts2Dll(): Promise<null> {
  return Promise.resolve(null);
}
