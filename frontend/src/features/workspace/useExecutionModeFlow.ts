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
import {
  createAndStartPlatformFlow,
  type PlatformRunProgressUpdate,
} from "../platform-run/createAndStartFlow.ts";
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
  if (
    preference?.default_execution_profile_id !== null &&
    typeof preference?.default_execution_profile_id !== "undefined"
  ) {
    const preferredProfile = availableProfiles.find(
      (profile) => profile.id === preference.default_execution_profile_id,
    );
    if (preferredProfile) {
      return preferredProfile.id;
    }
  }
  return availableProfiles.find((profile) => profile.recommended)?.id ?? availableProfiles[0]?.id ?? null;
}

interface ServerExecutionOptionsSnapshot {
  profiles: PlatformExecutionProfile[];
  preference: MyServerPreferenceView;
}

async function fetchServerExecutionOptions(): Promise<ServerExecutionOptionsSnapshot> {
  const [profileView, preference] = await Promise.all([listPlatformExecutionProfiles(), getMyServerPreferences()]);
  return {
    profiles: profileView.items,
    preference,
  };
}

function resolveServerSelectionNotice(
  profiles: PlatformExecutionProfile[],
  preference: MyServerPreferenceView,
  selectedProfileId: number | null,
): string | null {
  if (preference.default_execution_profile_id === null || preference.available) {
    return null;
  }

  const fallbackProfile = profiles.find((profile) => profile.id === selectedProfileId);
  return fallbackProfile
    ? `已保存的默认 Web 托管工作站配置当前不可用，本次已自动回退到 ${fallbackProfile.display_name}。`
    : "已保存的默认 Web 托管工作站配置当前不可用，而且暂时没有健康可用的平台执行配置。";
}

function resolveServerOptionsError(error: unknown) {
  return resolveErrorMessage(error, "读取平台执行配置失败");
}

function isWebBackendConnectionError(message: string) {
  return message.includes("无法连接Web 后端") || message.includes("无法连接 Web 后端");
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
  const [serverActionBusy, setServerActionBusy] = useState(false);
  const [serverActionProgress, setServerActionProgress] = useState<PlatformRunProgressUpdate | null>(null);

  function showExecutionNotice(title: string, message: string, tone?: "info" | "success" | "warning" | "error") {
    if (tone) {
      onStatusNotice?.({ title, message, tone });
      return;
    }
    onStatusNotice?.({ title, message, tone: "error" });
  }

  function applyServerExecutionOptions(
    snapshot: ServerExecutionOptionsSnapshot,
    options: { preserveSelectedProfile?: boolean } = {},
  ) {
    const preservedProfileId =
      options.preserveSelectedProfile &&
      selectedServerProfileId !== null &&
      snapshot.profiles.some((profile) => profile.id === selectedServerProfileId && profile.available)
        ? selectedServerProfileId
        : null;
    const nextProfileId = preservedProfileId ?? pickInitialServerProfileId(snapshot.profiles, snapshot.preference);
    setServerProfiles(snapshot.profiles);
    setServerPreference(snapshot.preference);
    setSelectedServerProfileId(nextProfileId);
    setRememberServerProfile(false);
    setServerProfilesError(null);
    setServerSelectionNotice(resolveServerSelectionNotice(snapshot.profiles, snapshot.preference, nextProfileId));
    return {
      ...snapshot,
      selectedProfileId: nextProfileId,
    };
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
      setServerActionBusy(false);
      setServerActionProgress(null);
      return;
    }

    let cancelled = false;
    setServerProfilesLoading(true);
    setServerProfilesError(null);

    void fetchServerExecutionOptions()
      .then((snapshot) => {
        if (cancelled) {
          return;
        }
        applyServerExecutionOptions(snapshot);
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setServerProfiles([]);
        setServerPreference(null);
        setSelectedServerProfileId(null);
        setRememberServerProfile(false);
        setServerProfilesError(resolveServerOptionsError(error));
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
      ...(capability.text_ai_available ? [] : (capability.text_ai_missing_reasons ?? [])),
      ...(request.requiresCodeAgent && !capability.code_agent_available
        ? (capability.code_agent_missing_reasons ?? [])
        : []),
      ...(request.requiresImageAi && !capability.image_ai_available ? (capability.image_ai_missing_reasons ?? []) : []),
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
    if (serverActionBusy) {
      return;
    }
    pendingStartConfirmation?.resolve(false);
    setPendingStartConfirmation(null);
    setPendingExecution(null);
  }

  function requestStartConfirmation(job: PlatformJobSummary) {
    return new Promise<boolean>((resolve) => {
      setPendingStartConfirmation({
        message: `已创建平台任务 #${job.id}。\n确认开始后会进入 Web 托管工作站队列，并按平台规则计费。是否继续开始？`,
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
    if (pendingExecution === null || serverActionBusy) {
      return;
    }

    const request = pendingExecution;
    if (!isAuthenticated) {
      handleGoLoginForServerExecution();
      return;
    }

    if ((request.serverUnsupportedReasons ?? []).length > 0) {
      showExecutionNotice(
        "Web 托管工作站暂不可用",
        request.serverUnsupportedReasons?.[0] ?? "当前任务暂不支持 Web 托管工作站",
        "warning",
      );
      return;
    }

    try {
      setServerActionBusy(true);
      setServerActionProgress({ stage: "checking_server", message: "正在确认 Web 后端和平台执行配置可用" });
      let refreshedOptions: ServerExecutionOptionsSnapshot & { selectedProfileId: number | null };
      try {
        refreshedOptions = applyServerExecutionOptions(await fetchServerExecutionOptions(), {
          preserveSelectedProfile: true,
        });
      } catch (error) {
        const message = resolveServerOptionsError(error);
        setServerProfiles([]);
        setServerPreference(null);
        setSelectedServerProfileId(null);
        setRememberServerProfile(false);
        setServerProfilesError(message);
        setServerSelectionNotice(null);
        showExecutionNotice("Web 后端不可用", message);
        return;
      }
      const selectedProfile = refreshedOptions.profiles.find(
        (profile) => profile.id === refreshedOptions.selectedProfileId && profile.available,
      );
      if (!selectedProfile) {
        showExecutionNotice("没有可用的平台执行配置", "当前没有健康可用的平台执行配置。请先检查服务器凭据和执行配置。", "warning");
        return;
      }
      showExecutionNotice("正在创建平台任务", "已开始提交 Web 托管工作站任务，请不要重复点击。", "info");
      if (rememberServerProfile && selectedProfile.id !== refreshedOptions.preference.default_execution_profile_id) {
        setServerActionProgress({ stage: "creating_job", message: "正在保存默认 Web 托管工作站配置" });
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
        serverUploads: request.serverUploads,
        serverWorkspaceProjectName: request.serverWorkspaceProjectName,
        selectedExecutionProfileId: selectedProfile.id,
        selectedRunnerType: selectedProfile.runner_type,
        selectedModel: selectedProfile.model,
        confirmStart: requestStartConfirmation,
        onProgress: setServerActionProgress,
      });
      setPendingExecution(null);
      setServerActionProgress(null);
      if (result.deferredNotice) {
        showExecutionNotice(
          result.deferredNotice.summary.title,
          describeDeferredReason(result.deferredNotice.summary),
          "warning",
        );
      }
      if (!result.startConfirmed) {
        showExecutionNotice("平台任务已创建", `任务 #${result.job.id} 已创建，暂未启动。`, "info");
      }
      navigate(`/me/jobs/${result.job.id}`);
    } catch (error) {
      const message = resolveErrorMessage(error, "创建平台任务失败");
      if (isWebBackendConnectionError(message)) {
        setServerProfilesError(message);
      }
      showExecutionNotice("创建平台任务失败", message);
    } finally {
      setServerActionBusy(false);
      setServerActionProgress(null);
    }
  }

  async function handleReloadServerProfiles() {
    if (pendingExecution === null || !isAuthenticated) {
      return;
    }
    setServerProfilesLoading(true);
    setServerProfilesError(null);
    try {
      applyServerExecutionOptions(await fetchServerExecutionOptions());
    } catch (error) {
      setServerProfiles([]);
      setServerPreference(null);
      setSelectedServerProfileId(null);
      setRememberServerProfile(false);
      setServerProfilesError(resolveServerOptionsError(error));
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
    serverActionBusy,
    serverActionProgress,
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
