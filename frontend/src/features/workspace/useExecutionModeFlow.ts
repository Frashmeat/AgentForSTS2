import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import {
  getMyServerPreferences,
  listPlatformExecutionProfiles,
  loadLocalAiCapabilityStatus,
  updateMyServerPreferences,
  type LocalAiCapabilityStatus,
  type MyServerPreferenceView,
} from "../../shared/api/index.ts";
import type { PlatformExecutionProfile, PlatformJobSummary } from "../../shared/api/platform.ts";
import { type DeferredExecutionSummary } from "../../shared/deferredExecution.ts";
import { resolveErrorMessage } from "../../shared/error.ts";
import { createAndStartPlatformFlow } from "../platform-run/createAndStartFlow.ts";
import type { PlatformExecutionRequest } from "../platform-run/types.ts";
import { buildWorkspacePath } from "./config.ts";

const PLATFORM_WORKFLOW_VERSION = "2026.04.04";

function describeDeferredReason(summary: DeferredExecutionSummary) {
  return summary.alertMessage;
}

function pickInitialServerProfileId(
  profiles: PlatformExecutionProfile[],
  preference: MyServerPreferenceView | null,
): number | null {
  const availableProfiles = profiles.filter((profile) => profile.available);
  if (availableProfiles.length === 0) {
    return null;
  }
  if (preference?.default_execution_profile_id !== null && typeof preference?.default_execution_profile_id !== "undefined") {
    const preferredProfile = availableProfiles.find(
      (profile) => profile.id === preference.default_execution_profile_id,
    );
    if (preferredProfile) {
      return preferredProfile.id;
    }
  }
  return availableProfiles.find((profile) => profile.recommended)?.id ?? availableProfiles[0]?.id ?? null;
}

export interface PendingExecutionRequest extends PlatformExecutionRequest {
  localAvailable: boolean;
  localUnavailableReasons: string[];
}

interface PendingStartConfirmation {
  message: string;
  resolve: (confirmed: boolean) => void;
}

interface UseExecutionModeFlowOptions {
  isAuthenticated: boolean;
  onStatusNotice?: (notice: {
    title: string;
    message?: string;
    tone?: "info" | "success" | "warning" | "error";
  }) => void;
}

export function useExecutionModeFlow({ isAuthenticated, onStatusNotice }: UseExecutionModeFlowOptions) {
  const navigate = useNavigate();
  const [pendingExecution, setPendingExecution] = useState<PendingExecutionRequest | null>(null);
  const [serverProfiles, setServerProfiles] = useState<PlatformExecutionProfile[]>([]);
  const [serverPreference, setServerPreference] = useState<MyServerPreferenceView | null>(null);
  const [selectedServerProfileId, setSelectedServerProfileId] = useState<number | null>(null);
  const [rememberServerProfile, setRememberServerProfile] = useState(false);
  const [serverProfilesLoading, setServerProfilesLoading] = useState(false);
  const [serverProfilesError, setServerProfilesError] = useState<string | null>(null);
  const [serverSelectionNotice, setServerSelectionNotice] = useState<string | null>(null);
  const [pendingStartConfirmation, setPendingStartConfirmation] = useState<PendingStartConfirmation | null>(null);

  function showExecutionNotice(
    title: string,
    message: string,
    tone?: "info" | "success" | "warning" | "error",
  ) {
    if (tone) {
      onStatusNotice?.({ title, message, tone });
      return;
    }
    onStatusNotice?.({ title, message, tone: "error" });
  }

  useEffect(() => {
    if (pendingExecution === null || !isAuthenticated) {
      setServerProfiles([]);
      setServerPreference(null);
      setSelectedServerProfileId(null);
      setRememberServerProfile(false);
      setServerProfilesLoading(false);
      setServerProfilesError(null);
      setServerSelectionNotice(null);
      return;
    }

    let cancelled = false;
    setServerProfilesLoading(true);
    setServerProfilesError(null);

    void Promise.all([listPlatformExecutionProfiles(), getMyServerPreferences()])
      .then(([profileView, preference]) => {
        if (cancelled) {
          return;
        }
        setServerProfiles(profileView.items);
        setServerPreference(preference);
        const nextProfileId = pickInitialServerProfileId(profileView.items, preference);
        setSelectedServerProfileId(nextProfileId);
        setRememberServerProfile(false);
        if (preference.default_execution_profile_id !== null && !preference.available) {
          const fallbackProfile = profileView.items.find((profile) => profile.id === nextProfileId);
          setServerSelectionNotice(
            fallbackProfile
              ? `已保存的默认服务器配置当前不可用，本次已自动回退到 ${fallbackProfile.display_name}。`
              : "已保存的默认服务器配置当前不可用，而且暂时没有健康可用的服务器执行配置。",
          );
        } else {
          setServerSelectionNotice(null);
        }
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setServerProfiles([]);
        setServerPreference(null);
        setSelectedServerProfileId(null);
        setRememberServerProfile(false);
        setServerProfilesError(error instanceof Error ? error.message : "读取服务器执行配置失败");
        setServerSelectionNotice(null);
      })
      .finally(() => {
        if (!cancelled) {
          setServerProfilesLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [isAuthenticated, pendingExecution]);

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
      serverUnsupportedReasons: request.serverUnsupportedReasons ?? [],
      localAvailable:
        capability.text_ai_available &&
        (!request.requiresCodeAgent || capability.code_agent_available) &&
        (!request.requiresImageAi || capability.image_ai_available),
      localUnavailableReasons,
    });
  }

  function closeExecutionDialog() {
    pendingStartConfirmation?.resolve(false);
    setPendingStartConfirmation(null);
    setPendingExecution(null);
  }

  function requestStartConfirmation(job: PlatformJobSummary) {
    return new Promise<boolean>((resolve) => {
      setPendingStartConfirmation({
        message: `已创建平台任务 #${job.id}。\n确认开始后会进入服务器队列，并按平台规则计费。是否继续开始？`,
        resolve,
      });
    });
  }

  function resolveStartConfirmation(confirmed: boolean) {
    if (!pendingStartConfirmation) {
      return;
    }
    pendingStartConfirmation.resolve(confirmed);
    setPendingStartConfirmation(null);
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

    if ((request.serverUnsupportedReasons ?? []).length > 0) {
      showExecutionNotice(
        "服务器模式暂不可用",
        request.serverUnsupportedReasons?.[0] ?? "当前任务暂不支持服务器模式",
        "warning",
      );
      return;
    }

    const selectedProfile = serverProfiles.find(
      (profile) => profile.id === selectedServerProfileId && profile.available,
    );
    if (!selectedProfile) {
      showExecutionNotice(
        "没有可用的服务器配置",
        serverProfilesError ?? "当前没有可用的服务器执行配置",
      );
      return;
    }

    try {
      if (
        rememberServerProfile &&
        selectedProfile.id !== serverPreference?.default_execution_profile_id
      ) {
        const updatedPreference = await updateMyServerPreferences({
          default_execution_profile_id: selectedProfile.id,
        });
        setServerPreference(updatedPreference);
        setServerSelectionNotice(null);
      }

      const result = await createAndStartPlatformFlow({
        jobType: request.jobType,
        workflowVersion: PLATFORM_WORKFLOW_VERSION,
        inputSummary: request.inputSummary,
        createdFrom: request.createdFrom,
        items: request.items,
        selectedExecutionProfileId: selectedProfile.id,
        selectedAgentBackend: selectedProfile.agent_backend,
        selectedModel: selectedProfile.model,
        confirmStart: requestStartConfirmation,
      });
      setPendingExecution(null);
      if (result.deferredNotice) {
        showExecutionNotice(
          result.deferredNotice.summary.title,
          describeDeferredReason(result.deferredNotice.summary),
          "warning",
        );
      }
      navigate(`/me/jobs/${result.job.id}`);
    } catch (error) {
      showExecutionNotice("创建平台任务失败", resolveErrorMessage(error, "创建平台任务失败"));
    }
  }

  async function handleReloadServerProfiles() {
    if (pendingExecution === null || !isAuthenticated) {
      return;
    }
    setServerProfilesLoading(true);
    setServerProfilesError(null);
    try {
      const [profileView, preference] = await Promise.all([
        listPlatformExecutionProfiles(),
        getMyServerPreferences(),
      ]);
      setServerProfiles(profileView.items);
      setServerPreference(preference);
      const nextProfileId = pickInitialServerProfileId(profileView.items, preference);
      setSelectedServerProfileId(nextProfileId);
      setRememberServerProfile(false);
      if (preference.default_execution_profile_id !== null && !preference.available) {
        const fallbackProfile = profileView.items.find((profile) => profile.id === nextProfileId);
        setServerSelectionNotice(
          fallbackProfile
            ? `已保存的默认服务器配置当前不可用，本次已自动回退到 ${fallbackProfile.display_name}。`
            : "已保存的默认服务器配置当前不可用，而且暂时没有健康可用的服务器执行配置。",
        );
      } else {
        setServerSelectionNotice(null);
      }
    } catch (error) {
      setServerProfiles([]);
      setServerPreference(null);
      setSelectedServerProfileId(null);
      setRememberServerProfile(false);
      setServerProfilesError(error instanceof Error ? error.message : "读取服务器执行配置失败");
      setServerSelectionNotice(null);
    } finally {
      setServerProfilesLoading(false);
    }
  }

  return {
    pendingExecution,
    pendingStartConfirmation,
    serverProfiles,
    serverProfilesLoading,
    serverProfilesError,
    serverSelectionNotice,
    selectedServerProfileId,
    rememberServerProfile,
    handleExecutionRequest,
    handleChooseLocalExecution,
    handleChooseServerExecution,
    handleGoLoginForServerExecution,
    closeExecutionDialog,
    confirmStartExecution: () => resolveStartConfirmation(true),
    cancelStartExecution: () => resolveStartConfirmation(false),
    handleReloadServerProfiles,
    setSelectedServerProfileId,
    setRememberServerProfile,
  };
}
