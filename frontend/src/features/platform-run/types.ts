import type { PlatformJobCreateItem } from "../../shared/api/platform.ts";

export type WorkspaceTab = "single" | "batch" | "edit" | "log";

export interface PlatformExecutionRequest {
  title: string;
  tab: WorkspaceTab;
  jobType: "single_generate" | "batch_generate" | "mod_edit" | "log_analysis";
  createdFrom: "single_asset" | "batch_generation" | "mod_editor" | "log_analysis";
  inputSummary: string;
  requiresCodeAgent: boolean;
  requiresImageAi: boolean;
  serverUnsupportedReasons?: string[];
  items: PlatformJobCreateItem[];
  runLocal: () => void;
}
