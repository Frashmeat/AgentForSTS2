import { BatchGenerationFeatureView } from "../batch-generation/view";
import { LogAnalysisFeatureView } from "../log-analysis/view";
import { ModEditorFeatureView } from "../mod-editor/view";
import type { WorkspaceTab } from "../platform-run/types.ts";
import { SingleAssetWorkspaceContainer } from "../single-asset/SingleAssetWorkspaceContainer.tsx";
import { useWorkspaceContext } from "./WorkspaceContext.tsx";

interface WorkspaceHomeProps {
  activeTab: WorkspaceTab;
}

export function WorkspaceHome({
  activeTab,
}: WorkspaceHomeProps) {
  const {
    knowledgeStatus,
    onOpenKnowledgeGuide,
    onOpenSettings,
    onRefreshKnowledge,
    onRequestExecution,
  } = useWorkspaceContext();
  return (
    <>
      {activeTab === "batch" && (
        <div className="px-4 py-4 sm:px-6 sm:py-6">
          <BatchGenerationFeatureView
            onRequestExecution={onRequestExecution}
            knowledgeStatus={knowledgeStatus}
            onOpenKnowledgeGuide={onOpenKnowledgeGuide}
            onOpenSettings={onOpenSettings}
          />
        </div>
      )}

      {activeTab === "edit" && (
        <div className="px-4 py-4 sm:px-6 sm:py-6">
          <ModEditorFeatureView
            onRequestExecution={onRequestExecution}
            knowledgeStatus={knowledgeStatus}
            onOpenKnowledgeGuide={onOpenKnowledgeGuide}
            onOpenSettings={onOpenSettings}
          />
        </div>
      )}

      {activeTab === "log" && (
        <div className="px-4 py-4 sm:px-6 sm:py-6">
          <LogAnalysisFeatureView
            onRequestExecution={onRequestExecution}
            knowledgeStatus={knowledgeStatus}
            onOpenKnowledgeGuide={onOpenKnowledgeGuide}
            onOpenSettings={onOpenSettings}
          />
        </div>
      )}

      {activeTab === "single" && (
        <SingleAssetWorkspaceContainer
          onRequestExecution={onRequestExecution}
          knowledgeStatus={knowledgeStatus}
          onRefreshKnowledge={onRefreshKnowledge}
          onOpenKnowledgeGuide={onOpenKnowledgeGuide}
          onOpenSettings={onOpenSettings}
        />
      )}
    </>
  );
}
