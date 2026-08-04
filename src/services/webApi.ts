import type {
  CurrentProject,
  FeatureContract,
  HealthReport,
  LogAnalyzeRequest,
  ModPlanRequest,
  ProjectBuildRequest,
  ProjectPackageRequest,
  RecentEntry,
  ResourcePrepareRequest,
  RunRecord,
  RunSummary,
  SettingsPatch,
  SettingsSnapshot,
  SingleGenerateRequest,
  BatchGenerateRequest,
  ComplexGenerateRequest,
  TruthStatus,
  ItemCapabilityCatalog,
  ItemDefinition,
  StoredItemDefinition,
} from "./tauriApi";
import type { ActionableFailure } from "./actionableFailure";
export { isActionableFailure, toActionableFailure } from "./actionableFailure";

const API_BASE = "/api";

export function invokeCommand<T>(command: string): Promise<T> {
  return Promise.reject(desktopOnly(command));
}

export function getHealth(): Promise<HealthReport> {
  return getJson("/health");
}
export function getFeatureCatalog(): Promise<FeatureContract[]> {
  return getJson("/features");
}

export function listRecentProjects(): Promise<RecentEntry[]> { return Promise.resolve([]); }
export function createProject(_parentDir: string, _name: string): Promise<CurrentProject> { return reject("createProject"); }
export function openProject(_path: string): Promise<CurrentProject> { return reject("openProject"); }
export function closeProject(): Promise<void> { return reject("closeProject"); }
export function currentProject(): Promise<CurrentProject | null> { return Promise.resolve(null); }
export function forgetRecentProject(_path: string): Promise<void> { return reject("forgetRecentProject"); }
export function getTruthStatus(): Promise<TruthStatus> { return reject("getTruthStatus"); }
export function importTruth(): Promise<TruthStatus> { return reject("importTruth"); }
export function getItemCapabilities(): Promise<ItemCapabilityCatalog> { return reject("getItemCapabilities"); }
export function listItemDefinitions(): Promise<StoredItemDefinition[]> { return Promise.resolve([]); }
export function getItemDefinition(_itemId: string, _definitionHash?: string): Promise<StoredItemDefinition> { return reject("getItemDefinition"); }
export function saveItemDefinition(_definition: ItemDefinition): Promise<StoredItemDefinition> { return reject("saveItemDefinition"); }
export function getRun(_runId: string): Promise<RunRecord> { return reject("getRun"); }
export function listRuns(): Promise<RunSummary[]> { return Promise.resolve([]); }
export function cancelRun(_runId: string): Promise<boolean> { return reject("cancelRun"); }
export function submitModPlan(_request: ModPlanRequest): Promise<string> { return reject("submitModPlan"); }
export function submitSingleGenerate(_request: SingleGenerateRequest): Promise<string> { return reject("submitSingleGenerate"); }
export function submitBatchGenerate(_request: BatchGenerateRequest): Promise<string> { return reject("submitBatchGenerate"); }
export function submitComplexGenerate(_request: ComplexGenerateRequest): Promise<string> { return reject("submitComplexGenerate"); }
export function submitLogAnalyze(_request: LogAnalyzeRequest): Promise<string> { return reject("submitLogAnalyze"); }
export function submitResourcePrepare(_request: ResourcePrepareRequest, _sourcePath?: string): Promise<string> { return reject("submitResourcePrepare"); }
export function submitProjectBuild(_request: ProjectBuildRequest = {}): Promise<string> { return reject("submitProjectBuild"); }
export function submitProjectPackage(_request: ProjectPackageRequest): Promise<string> { return reject("submitProjectPackage"); }
export function getSettingsSnapshot(): Promise<SettingsSnapshot> { return reject("getSettingsSnapshot"); }
export function saveSettingsPatch(_patch: SettingsPatch): Promise<SettingsSnapshot> { return reject("saveSettingsPatch"); }
export function openConfigInEditor(): Promise<string> { return reject("openConfigInEditor"); }

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`);
  if (!response.ok) throw httpFailure(response.status);
  return response.json() as Promise<T>;
}

function reject<T>(name: string): Promise<T> {
  return Promise.reject(desktopOnly(name));
}

function desktopOnly(_name: string): ActionableFailure {
  return {
    schemaVersion: 1,
    code: "web.desktop_only",
    category: "state",
    stage: "web.command",
    message: "This operation is available only in the desktop application.",
    action: "none",
    retryable: false,
  };
}

function httpFailure(status: number): ActionableFailure {
  return {
    schemaVersion: 1,
    code: status >= 500 ? "web.upstream_failed" : "web.request_invalid",
    category: status >= 500 ? "upstream" : "input",
    stage: "web.request",
    message: status >= 500 ? "The server could not complete the request." : "The server rejected the request.",
    action: status >= 500 ? "retry" : "none",
    retryable: status >= 500,
  };
}
