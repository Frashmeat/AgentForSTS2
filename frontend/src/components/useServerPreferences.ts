// SettingsPanel 的"服务器执行配置偏好"状态机封装。
// 只在 activeTab === "server" 且已登录时拉取与自动保存。
// 与 useDetectAppPathsTask / useRefreshKnowledgeTask 同模式：mountedRef 保护跨实例 + 自动淡出 notice。

import { useEffect, useState, type MutableRefObject } from "react";

import {
  getMyServerPreferences,
  listPlatformExecutionProfiles,
  type MyServerPreferenceView,
  updateMyServerPreferences,
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";
import type { PlatformExecutionProfile } from "../shared/api/platform.ts";
import { pickInitialServerProfileId } from "./settings-panel-helpers.ts";

export interface UseServerPreferencesResult {
  serverProfiles: PlatformExecutionProfile[];
  serverPreference: MyServerPreferenceView | null;
  selectedServerProfileId: number | null;
  serverLoading: boolean;
  serverSaving: boolean;
  serverError: string;
  serverNotice: string;
  serverSelectionDirty: boolean;
  selectProfile: (profileId: number | null) => void;
  saveServerPreference: (profileId: number | null) => Promise<void>;
}

export interface UseServerPreferencesOptions {
  mountedRef: MutableRefObject<boolean>;
  activeTab: "workspace" | "server";
  isAuthAvailable: boolean;
  isAuthenticated: boolean;
}

export function useServerPreferences({
  mountedRef,
  activeTab,
  isAuthAvailable,
  isAuthenticated,
}: UseServerPreferencesOptions): UseServerPreferencesResult {
  const [serverProfiles, setServerProfiles] = useState<PlatformExecutionProfile[]>([]);
  const [serverPreference, setServerPreference] = useState<MyServerPreferenceView | null>(null);
  const [selectedServerProfileId, setSelectedServerProfileId] = useState<number | null>(null);
  const [serverLoading, setServerLoading] = useState(false);
  const [serverSaving, setServerSaving] = useState(false);
  const [serverError, setServerError] = useState("");
  const [serverNotice, setServerNotice] = useState("");
  const [serverSelectionDirty, setServerSelectionDirty] = useState(false);

  // 切到 server 标签或登录态变化时（重新）拉取
  useEffect(() => {
    if (activeTab !== "server") {
      return;
    }
    if (!isAuthAvailable || !isAuthenticated) {
      setServerProfiles([]);
      setServerPreference(null);
      setSelectedServerProfileId(null);
      setServerError("");
      setServerNotice("");
      setServerSelectionDirty(false);
      return;
    }

    let cancelled = false;
    setServerLoading(true);
    setServerError("");
    void Promise.all([listPlatformExecutionProfiles(), getMyServerPreferences()])
      .then(([profileView, preference]) => {
        if (cancelled || !mountedRef.current) {
          return;
        }
        setServerProfiles(profileView.items);
        setServerPreference(preference);
        setSelectedServerProfileId(pickInitialServerProfileId(profileView.items, preference));
        setServerSelectionDirty(false);
      })
      .catch((error) => {
        if (cancelled || !mountedRef.current) {
          return;
        }
        setServerProfiles([]);
        setServerPreference(null);
        setSelectedServerProfileId(null);
        setServerError(resolveErrorMessage(error));
        setServerSelectionDirty(false);
      })
      .finally(() => {
        if (!cancelled) {
          setServerLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
    // mountedRef 是 ref 永不进 deps
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, isAuthAvailable, isAuthenticated]);

  // 选择变化触发的 500ms debounce 自动保存
  useEffect(() => {
    if (activeTab !== "server" || !isAuthAvailable || !isAuthenticated || serverSaving || !serverSelectionDirty) {
      return;
    }

    const timer = setTimeout(() => {
      void saveServerPreference(selectedServerProfileId ?? null);
    }, 500);

    return () => {
      clearTimeout(timer);
    };
    // saveServerPreference 是组件内闭包；deps 已覆盖触发条件。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, isAuthAvailable, isAuthenticated, selectedServerProfileId, serverSaving, serverSelectionDirty]);

  // 保存成功 notice 2.6s 后自动淡出
  useEffect(() => {
    if (!serverNotice) {
      return;
    }
    const timer = setTimeout(() => {
      setServerNotice("");
    }, 2600);
    return () => clearTimeout(timer);
  }, [serverNotice]);

  function selectProfile(profileId: number | null) {
    setSelectedServerProfileId(profileId);
    setServerSelectionDirty(true);
  }

  async function saveServerPreference(profileId: number | null) {
    if (!isAuthAvailable || !isAuthenticated) {
      return;
    }
    setServerSaving(true);
    setServerError("");
    setServerNotice("");
    try {
      const updated = await updateMyServerPreferences({
        default_execution_profile_id: profileId,
      });
      if (!mountedRef.current) {
        return;
      }
      setServerPreference(updated);
      setSelectedServerProfileId(profileId);
      setServerSelectionDirty(false);
      setServerNotice(profileId === null ? "已自动清空默认服务器配置" : "已自动保存默认服务器配置");
    } catch (error) {
      if (mountedRef.current) {
        setServerError(resolveErrorMessage(error));
        setServerSelectionDirty(false);
      }
      throw error;
    } finally {
      if (mountedRef.current) {
        setServerSaving(false);
      }
    }
  }

  return {
    serverProfiles,
    serverPreference,
    selectedServerProfileId,
    serverLoading,
    serverSaving,
    serverError,
    serverNotice,
    serverSelectionDirty,
    selectProfile,
    saveServerPreference,
  };
}
