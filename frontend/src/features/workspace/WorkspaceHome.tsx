import { BatchGenerationFeatureView } from "../batch-generation/view";
import { LogAnalysisFeatureView } from "../log-analysis/view";
import { ModEditorFeatureView } from "../mod-editor/view";
import type { WorkspaceTab } from "../platform-run/types.ts";
import { SingleAssetWorkspaceContainer } from "../single-asset/SingleAssetWorkspaceContainer.tsx";

interface WorkspaceHomeProps {
  activeTab: WorkspaceTab;
}

export function WorkspaceHome({
  activeTab,
}: WorkspaceHomeProps) {
  return (
    <>
      {activeTab === "batch" && (
        <div className="px-4 py-4 sm:px-6 sm:py-6">
          <BatchGenerationFeatureView />
        </div>
      )}

      {activeTab === "edit" && (
        <div className="px-4 py-4 sm:px-6 sm:py-6">
          <ModEditorFeatureView />
        </div>
      )}

      {activeTab === "log" && (
        <div className="px-4 py-4 sm:px-6 sm:py-6">
          <LogAnalysisFeatureView />
        </div>
      )}

      {activeTab === "single" && (
        <SingleAssetWorkspaceContainer />
      )}
    </>
  );
}
