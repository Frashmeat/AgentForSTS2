import { useState } from "react";
import { useNavigate } from "react-router-dom";

import {
  loadLocalAiCapabilityStatus,
  type LocalAiCapabilityStatus,
} from "../../shared/api/index.ts";
import { createAndStartPlatformFlow } from "../platform-run/createAndStartFlow.ts";
import type { PlatformExecutionRequest, WorkspaceTab } from "../platform-run/types.ts";
import { buildWorkspacePath } from "./config.ts";

const PLATFORM_WORKFLOW_VERSION = "2026.04.04";

function describeDeferredReason(reasonCode: string, reasonMessage: string) {
  switch (reasonCode) {
    case "local_project_root_required":
      return `任务已创建，但当前仍依赖本地项目目录，服务器还不能直接继续执行。\n${reasonMessage}`;
    case "uploaded_asset_not_persisted":
      return `任务已创建，但当前仍依赖本地上传资产，服务器还不能直接继续执行。\n${reasonMessage}`;
    case "workflow_not_registered":
      return `任务已创建，但当前任务类型尚未接入服务器执行器。\n${reasonMessage}`;
    default:
      return `任务已创建，但当前没有进入真实服务器执行。\n${reasonMessage}`;
  }
}

export interface PendingExecutionRequest extends PlatformExecutionRequest {
  localAvailable: boolean;
  localUnavailableReasons: string[];
}

interface UseExecutionModeFlowOptions {
  isAuthenticated: boolean;
}

export function useExecutionModeFlow({ isAuthenticated }: UseExecutionModeFlowOptions) {
  const navigate = useNavigate();
  const [pendingExecution, setPendingExecution] = useState<PendingExecutionRequest | null>(null);

  async function handleExecutionRequest(request: PlatformExecutionRequest) {
    let capability: LocalAiCapabilityStatus;
    try {
      capability = await loadLocalAiCapabilityStatus();
    } catch {
      capability = {
        text_ai_available: false,
        code_agent_available: false,
        image_ai_available: false,
        text_ai_missing_reasons: ["无法读取本机配置状态，请检查工作站后端是否正常运行。"],
        code_agent_missing_reasons: ["无法读取本机代码代理状态，请检查工作站后端是否正常运行。"],
        image_ai_missing_reasons: [],
      };
    }

    const localUnavailableReasons = [
      ...(capability.text_ai_available ? [] : capability.text_ai_missing_reasons ?? []),
      ...(request.requiresCodeAgent && !capability.code_agent_available ? capability.code_agent_missing_reasons ?? [] : []),
      ...(
        request.requiresImageAi && !capability.image_ai_available
          ? capability.image_ai_missing_reasons ?? []
          : []
      ),
    ];

    setPendingExecution({
      ...request,
      localAvailable:
        capability.text_ai_available &&
        (!request.requiresCodeAgent || capability.code_agent_available) &&
        (!request.requiresImageAi || capability.image_ai_available),
      localUnavailableReasons,
    });
  }

  function closeExecutionDialog() {
    setPendingExecution(null);
  }

  function handleChooseLocalExecution() {
    if (pendingExecution === null) {
      return;
    }
    const request = pendingExecution;
    setPendingExecution(null);
    request.runLocal();
  }

  function handleGoLoginForServerExecution() {
    if (pendingExecution === null) {
      return;
    }
    const request = pendingExecution;
    setPendingExecution(null);
    navigate("/auth/login", {
      replace: true,
      state: {
        redirectTo: buildWorkspacePath(request.tab),
      },
    });
  }

  async function handleChooseServerExecution() {
    if (pendingExecution === null) {
      return;
    }

    const request = pendingExecution;
    if (!isAuthenticated) {
      handleGoLoginForServerExecution();
      return;
    }

    setPendingExecution(null);
    try {
      const result = await createAndStartPlatformFlow({
        jobType: request.jobType,
        workflowVersion: PLATFORM_WORKFLOW_VERSION,
        inputSummary: request.inputSummary,
        createdFrom: request.createdFrom,
        items: request.items,
        confirmStart(job) {
          return window.confirm(
            `已创建平台任务 #${job.id}。确认开始后会进入服务器队列，并按平台规则计费。是否继续开始？`,
          );
        },
      });
      if (result.deferredNotice) {
        window.alert(describeDeferredReason(result.deferredNotice.reasonCode, result.deferredNotice.reasonMessage));
      }
      navigate(`/me/jobs/${result.job.id}`);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : "创建平台任务失败");
    }
  }

  return {
    pendingExecution,
    handleExecutionRequest,
    handleChooseLocalExecution,
    handleChooseServerExecution,
    handleGoLoginForServerExecution,
    closeExecutionDialog,
  };
}
