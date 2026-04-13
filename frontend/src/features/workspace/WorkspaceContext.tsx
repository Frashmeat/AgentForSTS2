import { createContext, useContext, type ReactNode } from "react";

import type { KnowledgeStatus } from "../../shared/api/index.ts";
import type {
  WorkspaceExecutionRequestHandler,
  WorkspaceFeatureAdapterProps,
  WorkspaceFeatureProps,
} from "./types.ts";

export interface WorkspaceContextValue {
  knowledgeStatus: KnowledgeStatus | null;
  onRequestExecution: WorkspaceExecutionRequestHandler;
  onOpenKnowledgeGuide: () => void;
  onOpenSettings: () => void;
  onRefreshKnowledge: () => void;
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null);

export function WorkspaceProvider({
  value,
  children,
}: {
  value: WorkspaceContextValue;
  children: ReactNode;
}) {
  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspaceContext() {
  const context = useContext(WorkspaceContext);
  if (!context) {
    throw new Error("WorkspaceProvider is required");
  }
  return context;
}

export function useOptionalWorkspaceContext() {
  return useContext(WorkspaceContext);
}

export function useResolvedWorkspaceFeatureProps(
  props: WorkspaceFeatureAdapterProps,
): WorkspaceFeatureProps {
  const workspace = useOptionalWorkspaceContext();
  return {
    onRequestExecution: props.onRequestExecution ?? workspace?.onRequestExecution,
    knowledgeStatus: props.knowledgeStatus ?? workspace?.knowledgeStatus ?? null,
    onOpenKnowledgeGuide: props.onOpenKnowledgeGuide ?? workspace?.onOpenKnowledgeGuide ?? (() => {}),
    onOpenSettings: props.onOpenSettings ?? workspace?.onOpenSettings ?? (() => {}),
  };
}
