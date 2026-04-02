import { useState } from "react";

import { createProjectFromRoot } from "./projectCreation.ts";

interface UseProjectCreationOptions {
  onProjectCreated?: (projectPath: string) => void;
}

export function useProjectCreation(options: UseProjectCreationOptions = {}) {
  const { onProjectCreated } = options;
  const [projectCreateBusy, setProjectCreateBusy] = useState(false);
  const [projectCreateMessage, setProjectCreateMessage] = useState<string | null>(null);
  const [projectCreateError, setProjectCreateError] = useState<string | null>(null);

  function clearProjectCreationFeedback() {
    setProjectCreateMessage(null);
    setProjectCreateError(null);
  }

  function resetProjectCreationState() {
    setProjectCreateBusy(false);
    clearProjectCreationFeedback();
  }

  async function createProjectAtRoot(projectRoot: string) {
    setProjectCreateBusy(true);
    clearProjectCreationFeedback();
    try {
      const result = await createProjectFromRoot(projectRoot);
      onProjectCreated?.(result.project_path);
      setProjectCreateMessage(`项目已创建：${result.project_path}`);
      return result;
    } catch (error) {
      setProjectCreateError(error instanceof Error ? error.message : String(error));
      throw error;
    } finally {
      setProjectCreateBusy(false);
    }
  }

  return {
    projectCreateBusy,
    projectCreateMessage,
    projectCreateError,
    clearProjectCreationFeedback,
    resetProjectCreationState,
    createProjectAtRoot,
  };
}
