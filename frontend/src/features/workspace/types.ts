import type { KnowledgeStatus } from "../../shared/api/index.ts";
import type { StatusNoticeItem } from "../../components/StatusNotice.tsx";
import type { PlatformExecutionRequest } from "../platform-run/types.ts";

export type WorkspaceExecutionRequestHandler = (request: PlatformExecutionRequest) => void | Promise<void>;
export type WorkspaceStatusNoticeHandler = (notice: Omit<StatusNoticeItem, "id">) => void;

export interface WorkspaceFeatureProps {
  onRequestExecution?: WorkspaceExecutionRequestHandler;
  onStatusNotice: WorkspaceStatusNoticeHandler;
  knowledgeStatus: KnowledgeStatus | null;
  onOpenKnowledgeGuide: () => void;
  onOpenSettings: () => void;
}

export interface WorkspaceFeatureAdapterProps {
  onRequestExecution?: WorkspaceExecutionRequestHandler;
  onStatusNotice?: WorkspaceStatusNoticeHandler;
  knowledgeStatus?: KnowledgeStatus | null;
  onOpenKnowledgeGuide?: () => void;
  onOpenSettings?: () => void;
}
