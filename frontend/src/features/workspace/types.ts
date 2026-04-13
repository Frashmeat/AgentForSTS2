import type { KnowledgeStatus } from "../../shared/api/index.ts";
import type { PlatformExecutionRequest } from "../platform-run/types.ts";

export type WorkspaceExecutionRequestHandler = (request: PlatformExecutionRequest) => void | Promise<void>;

export interface WorkspaceFeatureProps {
  onRequestExecution?: WorkspaceExecutionRequestHandler;
  knowledgeStatus: KnowledgeStatus | null;
  onOpenKnowledgeGuide: () => void;
  onOpenSettings: () => void;
}

export interface WorkspaceFeatureAdapterProps {
  onRequestExecution?: WorkspaceExecutionRequestHandler;
  knowledgeStatus?: KnowledgeStatus | null;
  onOpenKnowledgeGuide?: () => void;
  onOpenSettings?: () => void;
}
