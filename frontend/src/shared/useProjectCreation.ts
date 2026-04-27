import { useState } from "react";

import type { StatusNoticeItem } from "../components/StatusNotice.tsx";
import { resolveErrorMessage } from "./error.ts";
import { createProjectFromRoot } from "./projectCreation.ts";

type StatusNoticeHandler = (notice: Omit<StatusNoticeItem, "id">) => void;

interface UseProjectCreationOptions {
  onProjectCreated?: (projectPath: string) => void;
  onStatusNotice?: StatusNoticeHandler;
}

export function useProjectCreation(options: UseProjectCreationOptions = {}) {
  const { onProjectCreated, onStatusNotice } = options;
  const [projectCreateBusy, setProjectCreateBusy] = useState(false);

  function clearProjectCreationFeedback() {}

  function resetProjectCreationState() {
    setProjectCreateBusy(false);
  }

  async function createProjectAtRoot(projectRoot: string) {
    setProjectCreateBusy(true);
    try {
      const result = await createProjectFromRoot(projectRoot);
      onProjectCreated?.(result.project_path);
      onStatusNotice?.({
        title: "项目已创建",
        message: result.project_path,
        tone: "success",
      });
      return result;
    } catch (error) {
      const message = resolveErrorMessage(error);
      onStatusNotice?.({
        title: "创建项目失败",
        message,
        tone: "error",
      });
      throw error;
    } finally {
      setProjectCreateBusy(false);
    }
  }

  return {
    projectCreateBusy,
    clearProjectCreationFeedback,
    resetProjectCreationState,
    createProjectAtRoot,
  };
}
