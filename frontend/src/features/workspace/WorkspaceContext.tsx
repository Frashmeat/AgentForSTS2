import { createContext, useContext, type ReactNode } from "react";

import type { KnowledgeStatus } from "../../shared/api/index.ts";
import type { WorkspaceExecutionRequestHandler } from "./types.ts";

interface WorkspaceContextValue {
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
