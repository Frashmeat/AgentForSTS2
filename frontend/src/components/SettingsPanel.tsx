import { useEffect, useRef, useState } from "react";
import { X, FolderOpen, Cpu, Cloud, Gamepad2, Image, Search } from "lucide-react";
import { Link } from "react-router-dom";
import {
  checkKnowledgeStatus,
  cancelDetectAppPathsTask,
  getMyServerPreferences,
  getDetectAppPathsTask,
  getLatestDetectAppPathsTask,
  getLatestRefreshKnowledgeTask,
  getRefreshKnowledgeTask,
  listPlatformExecutionProfiles,
  loadAppConfig,
  loadKnowledgeStatus,
  pickAppPath,
  startDetectAppPaths,
  startRefreshKnowledgeTask,
  testImageGenerationConfig,
  type KnowledgeStatus,
  type MyServerPreferenceView,
  updateAppConfig,
  updateMyServerPreferences,
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";
import type { PlatformExecutionProfile } from "../shared/api/platform.ts";
import { useSession } from "../shared/session/hooks.ts";
import { KnowledgeGuideDialog } from "./KnowledgeGuideDialog.tsx";
import { StatusNotice, StatusNoticeStack, type StatusNoticeItem } from "./StatusNotice.tsx";
import { createSettingsPickPathRequest, type SettingsPathField } from "./settingsPathPicker.ts";
import { Field, ProgressBar, SGroup } from "./SettingsLayout.tsx";
import {
  PROVIDER_MODELS,
  getKnowledgeUpdateProgress,
  inputCls,
  pickInitialServerProfileId,
  readonlyInputCls,
  selectCls,
} from "./settings-panel-helpers.ts";

interface SettingsPanelProps {
  mode?: "drawer" | "page";
  onClose?: () => void;
  onKnowledgeStatusChange?: (status: KnowledgeStatus) => void;
}

// SettingsPanel 内 cfg 是后端 config.json 的镜像，结构嵌套且 schema 在后端松散维护。
// 严格化需要先把 AppConfig 子结构补齐（审查 #8 前端拆分时一起做），本轮先承认债务保留 any。
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function buildConfigSaveBody(cfg: any, llmKey: string, imgKey: string, imgSecret: string) {
  const body = structuredClone(cfg);
  if (llmKey.trim()) body.llm.api_key = llmKey.trim();
  if (imgKey.trim()) body.image_gen.api_key = imgKey.trim();
  if (imgSecret.trim()) body.image_gen.api_secret = imgSecret.trim();
  return body;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function buildConfigSaveSignature(cfg: any, llmKey: string, imgKey: string, imgSecret: string) {
  return JSON.stringify(buildConfigSaveBody(cfg, llmKey, imgKey, imgSecret));
}

let retainedDetectionTaskId = "";
let retainedKnowledgeTaskId = "";

export function SettingsPanel({ mode = "drawer", onClose, onKnowledgeStatusChange }: SettingsPanelProps) {
  const { isAuthAvailable, isAuthenticated } = useSession();
  const [activeTab, setActiveTab] = useState<"workspace" | "server">("workspace");
  // 同上，cfg 留作 any（审查 #8 前端拆分时严格化）
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [cfg, setCfg] = useState<any>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [saveNotice, setSaveNotice] = useState("");
  const [hasSavedConfigOnce, setHasSavedConfigOnce] = useState(false);
  const [detecting, setDetecting] = useState(Boolean(retainedDetectionTaskId));
  const [detectionTaskId, setDetectionTaskIdState] = useState(retainedDetectionTaskId);
  const [detectionStep, setDetectionStep] = useState("");
  const [pathNotes, setPathNotes] = useState<string[]>([]);
  const [knowledgeStatus, setKnowledgeStatus] = useState<KnowledgeStatus | null>(null);
  const [knowledgeTaskId, setKnowledgeTaskIdState] = useState(retainedKnowledgeTaskId);
  const [knowledgeChecking, setKnowledgeChecking] = useState(false);
  const [knowledgeStep, setKnowledgeStep] = useState("");
  const [knowledgeNotes, setKnowledgeNotes] = useState<string[]>([]);
  const [knowledgeError, setKnowledgeError] = useState("");
  const [knowledgeGuideOpen, setKnowledgeGuideOpen] = useState(false);
  const [llmKey, setLlmKey] = useState("");
  const [imgKey, setImgKey] = useState("");
  const [imgSecret, setImgSecret] = useState("");
  const [imageTestLoading, setImageTestLoading] = useState(false);
  const [serverProfiles, setServerProfiles] = useState<PlatformExecutionProfile[]>([]);
  const [serverPreference, setServerPreference] = useState<MyServerPreferenceView | null>(null);
  const [selectedServerProfileId, setSelectedServerProfileId] = useState<number | null>(null);
  const [serverLoading, setServerLoading] = useState(false);
  const [serverSaving, setServerSaving] = useState(false);
  const [serverError, setServerError] = useState("");
  const [serverNotice, setServerNotice] = useState("");
  const [serverSelectionDirty, setServerSelectionDirty] = useState(false);
  const mountedRef = useRef(false);
  const lastSavedConfigSignatureRef = useRef("");
  const configSaveSignature = cfg ? buildConfigSaveSignature(cfg, llmKey, imgKey, imgSecret) : "";
  const configDirty = Boolean(cfg) && configSaveSignature !== lastSavedConfigSignatureRef.current;

  function setDetectionTaskId(taskId: string) {
    retainedDetectionTaskId = taskId;
    if (mountedRef.current) {
      setDetectionTaskIdState(taskId);
    }
  }

  function setKnowledgeTaskId(taskId: string) {
    retainedKnowledgeTaskId = taskId;
    if (mountedRef.current) {
      setKnowledgeTaskIdState(taskId);
    }
  }

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    function isActiveTask(status: string) {
      return status === "pending" || status === "running";
    }

    loadAppConfig().then((loaded) => {
      if (!mountedRef.current) {
        return;
      }
      setCfg(loaded);
      lastSavedConfigSignatureRef.current = buildConfigSaveSignature(loaded, "", "", "");
    });
    loadKnowledgeStatus()
      .then((status) => {
        if (!mountedRef.current) {
          return;
        }
        setKnowledgeStatus(status);
        onKnowledgeStatusChange?.(status);
      })
      .catch((error) => {
        if (!mountedRef.current) {
          return;
        }
        setKnowledgeError(resolveErrorMessage(error));
      });
    getLatestDetectAppPathsTask()
      .then((task) => {
        if (!mountedRef.current || !task || retainedDetectionTaskId || !isActiveTask(task.status)) {
          return;
        }
        setDetecting(true);
        setDetectionStep(task.current_step || "");
        setPathNotes(task.notes ?? []);
        setDetectionTaskId(task.task_id);
      })
      .catch(() => {
        // 恢复入口失败不阻塞设置面板初始化。
      });
    getLatestRefreshKnowledgeTask()
      .then((task) => {
        if (!mountedRef.current || !task || retainedKnowledgeTaskId || !isActiveTask(task.status)) {
          return;
        }
        setKnowledgeError("");
        setKnowledgeStep(task.current_step || "");
        setKnowledgeNotes(task.notes ?? []);
        setKnowledgeTaskId(task.task_id);
      })
      .catch(() => {
        // 恢复入口失败不阻塞设置面板初始化。
      });
    // mount-only：仅初始化时尝试恢复最近一次任务，onKnowledgeStatusChange 故意不进 deps，
    // 否则父级 callback 引用变化会触发重复恢复。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
  }, [activeTab, isAuthAvailable, isAuthenticated]);

  useEffect(() => {
    if (!cfg || saving || !configDirty || saveError) {
      return;
    }

    const timer = setTimeout(() => {
      void save();
    }, 700);

    return () => {
      clearTimeout(timer);
    };
    // save 是组件内闭包函数，每次 render 都重新创建；放进 deps 会让 debounce 永远重置。
    // 这里依赖 state 而非 callback —— 真正影响是否触发 save 的是 cfg/saving/configDirty 等。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cfg, saving, configDirty, configSaveSignature, saveError]);

  useEffect(() => {
    if (!saveNotice) {
      return;
    }

    const timer = setTimeout(() => {
      setSaveNotice("");
    }, 2600);

    return () => {
      clearTimeout(timer);
    };
  }, [saveNotice]);

  useEffect(() => {
    if (!serverNotice) {
      return;
    }

    const timer = setTimeout(() => {
      setServerNotice("");
    }, 2600);

    return () => {
      clearTimeout(timer);
    };
  }, [serverNotice]);

  useEffect(() => {
    if (activeTab !== "server" || !isAuthAvailable || !isAuthenticated || serverSaving || !serverSelectionDirty) {
      return;
    }

    const timer = setTimeout(() => {
      void handleSaveServerPreference(selectedServerProfileId ?? null);
    }, 500);

    return () => {
      clearTimeout(timer);
    };
    // 同上：handleSaveServerPreference 是组件内闭包，依赖列表里的 state 已能覆盖触发条件。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, isAuthAvailable, isAuthenticated, selectedServerProfileId, serverSaving, serverSelectionDirty]);

  useEffect(() => {
    if (!detectionTaskId) {
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function pollTask() {
      try {
        const snapshot = await getDetectAppPathsTask(detectionTaskId);
        if (cancelled || !mountedRef.current) {
          return;
        }

        setDetectionStep(snapshot.current_step || "");
        setPathNotes(snapshot.notes ?? []);
        if (snapshot.sts2_path) {
          set(["sts2_path"], snapshot.sts2_path);
        }
        if (snapshot.godot_exe_path) {
          set(["godot_exe_path"], snapshot.godot_exe_path);
        }

        if (snapshot.status === "running" || snapshot.status === "pending") {
          timer = setTimeout(pollTask, 500);
          return;
        }

        if (snapshot.status === "failed") {
          const message = snapshot.error?.trim() || "检测失败，请使用右侧选择按钮手动指定路径";
          setPathNotes((prev) => [...(snapshot.notes ?? prev), `检测失败：${message}`]);
        }

        setDetecting(false);
        setDetectionTaskId("");
      } catch (error) {
        if (cancelled) {
          return;
        }
        setDetecting(false);
        setDetectionTaskId("");
        setDetectionStep("");
        setPathNotes([`检测失败：${resolveErrorMessage(error)}`]);
      }
    }

    void pollTask();

    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [detectionTaskId]);

  useEffect(() => {
    if (!knowledgeTaskId) {
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function pollRefreshTask() {
      try {
        const snapshot = await getRefreshKnowledgeTask(knowledgeTaskId);
        if (cancelled) {
          return;
        }

        setKnowledgeStep(snapshot.current_step || "");
        setKnowledgeNotes(snapshot.notes ?? []);

        if (snapshot.status === "running" || snapshot.status === "pending") {
          timer = setTimeout(pollRefreshTask, 800);
          return;
        }

        if (snapshot.status === "failed") {
          setKnowledgeError(snapshot.error?.trim() || "知识库更新失败");
        }

        const status = await loadKnowledgeStatus();
        if (!cancelled && mountedRef.current) {
          setKnowledgeStatus(status);
          onKnowledgeStatusChange?.(status);
        }
        setKnowledgeTaskId("");
      } catch (error) {
        if (cancelled || !mountedRef.current) {
          return;
        }
        setKnowledgeError(resolveErrorMessage(error));
        setKnowledgeTaskId("");
      }
    }

    void pollRefreshTask();

    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
    // 仅由 knowledgeTaskId 驱动轮询；onKnowledgeStatusChange 是父级 callback，故意省略避免重复触发。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [knowledgeTaskId]);

  useEffect(() => {
    if (detecting || detectionTaskId || pathNotes.length === 0) {
      return;
    }

    const hasFailure = pathNotes.some((note) => note.includes("失败"));
    if (hasFailure) {
      return;
    }

    const timer = setTimeout(() => {
      setDetectionStep("");
      setPathNotes([]);
    }, 3200);

    return () => {
      clearTimeout(timer);
    };
  }, [detecting, detectionTaskId, pathNotes]);

  useEffect(() => {
    if (knowledgeChecking || knowledgeTaskId || knowledgeError || knowledgeNotes.length === 0) {
      return;
    }

    const timer = setTimeout(() => {
      setKnowledgeStep("");
      setKnowledgeNotes([]);
    }, 3200);

    return () => {
      clearTimeout(timer);
    };
  }, [knowledgeChecking, knowledgeTaskId, knowledgeError, knowledgeNotes]);

  function set(path: string[], value: string | number) {
    setSaveError("");
    setSaveNotice("");
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    setCfg((prev: any) => {
      if (!prev) {
        return prev;
      }
      let cur = prev;
      for (let i = 0; i < path.length - 1; i++) cur = cur[path[i]];
      if (cur[path[path.length - 1]] === value) {
        return prev;
      }
      const next = structuredClone(prev);
      let nextCur = next;
      for (let i = 0; i < path.length - 1; i++) nextCur = nextCur[path[i]];
      nextCur[path[path.length - 1]] = value;
      return next;
    });
  }

  function handleProviderChange(provider: string) {
    setSaveError("");
    setSaveNotice("");
    const models = PROVIDER_MODELS[provider] ?? [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    setCfg((prev: any) => {
      const next = structuredClone(prev);
      next.image_gen.provider = provider;
      if (models.length > 0 && !models.includes(next.image_gen.model)) {
        next.image_gen.model = models[0];
      }
      return next;
    });
  }

  async function detectPaths() {
    setDetecting(true);
    setDetectionStep("准备启动检测任务");
    setPathNotes([]);
    try {
      const task = await startDetectAppPaths();
      if (!mountedRef.current) {
        retainedDetectionTaskId = task.task_id;
        return;
      }
      setDetectionTaskId(task.task_id);
      setDetectionStep(task.current_step || "检测中");
      setPathNotes(task.notes ?? []);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setDetectionStep("");
      setPathNotes([`检测失败：${resolveErrorMessage(error)}`]);
      setDetecting(false);
    }
  }

  async function cancelDetectPaths() {
    if (!detectionTaskId) return;
    try {
      const snapshot = await cancelDetectAppPathsTask(detectionTaskId);
      if (!mountedRef.current) {
        return;
      }
      setDetectionStep(snapshot.current_step || "检测已取消");
      setPathNotes(snapshot.notes ?? ["检测已取消"]);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setPathNotes([`取消检测失败：${resolveErrorMessage(error)}`]);
    }
  }

  async function choosePath(field: SettingsPathField) {
    if (!cfg) return;
    const request = createSettingsPickPathRequest(field, String(cfg[field] ?? ""));
    try {
      const result = await pickAppPath(request);
      if (!mountedRef.current) {
        return;
      }
      if (!result.path) {
        return;
      }
      set([field], result.path);
      setPathNotes([`✓ ${request.title}: ${result.path}`]);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setPathNotes([`${request.title}失败：${resolveErrorMessage(error)}`]);
    }
  }

  async function save() {
    if (!cfg) {
      return;
    }
    const body = buildConfigSaveBody(cfg, llmKey, imgKey, imgSecret);
    const signature = JSON.stringify(body);
    if (signature === lastSavedConfigSignatureRef.current) {
      return;
    }
    setSaving(true);
    setSaveError("");
    setSaveNotice("");
    try {
      await updateAppConfig(body);
      lastSavedConfigSignatureRef.current = signature;
      if (!mountedRef.current) {
        return;
      }
      setHasSavedConfigOnce(true);
      setSaveNotice("已自动保存工作区设置");
    } catch (error) {
      const message = resolveErrorMessage(error) || "保存设置失败";
      if (mountedRef.current) {
        setSaveError(message);
      }
      throw error;
    } finally {
      if (mountedRef.current) {
        setSaving(false);
      }
    }
  }

  async function handleTestImageGenerationConfig() {
    if (imageTestLoading) {
      return;
    }

    setImageTestLoading(true);
    setSaveError("");
    setSaveNotice("");

    try {
      const result = await testImageGenerationConfig();
      if (!mountedRef.current) {
        return;
      }
      const sizeText = result.size ? `（${result.size[0]} x ${result.size[1]}）` : "";
      setSaveNotice(`生图配置测试成功${sizeText}`);
    } catch (error) {
      if (mountedRef.current) {
        setSaveError(resolveErrorMessage(error) || "生图配置测试失败");
      }
    } finally {
      if (mountedRef.current) {
        setImageTestLoading(false);
      }
    }
  }

  const currentProvider = cfg?.image_gen?.provider ?? "bfl";
  const models = PROVIDER_MODELS[currentProvider] ?? [];
  const workspaceSaveStatus = saveError
    ? ""
    : saving
      ? "正在自动保存工作区设置…"
      : configDirty
        ? "检测到修改，停止输入后会自动保存"
        : saveNotice || (hasSavedConfigOnce ? "工作区设置已自动保存" : "修改后会自动保存");
  const serverStatusText = serverError
    ? ""
    : serverSaving
      ? "正在自动保存默认服务器配置…"
      : serverSelectionDirty
        ? "检测到修改，稍后会自动保存默认服务器配置"
        : serverNotice;
  const missingPaths = cfg && (!cfg.default_project_root || !cfg.sts2_path);
  const knowledgeBusy = knowledgeChecking || Boolean(knowledgeTaskId);
  const knowledgeCheckProgress = knowledgeChecking ? 100 : 0;
  const knowledgeUpdateProgress = getKnowledgeUpdateProgress(knowledgeStep, Boolean(knowledgeTaskId));
  const pathFailureNote = pathNotes.find((note) => note.includes("失败")) ?? "";
  const pathSuccessNotes = pathNotes.filter((note) => note.startsWith("✓"));
  const pathNoticeMessage = detecting
    ? detectionStep || "正在自动检测项目路径"
    : pathFailureNote || pathSuccessNotes[0] || (missingPaths ? "请补充默认项目目录和 STS2 游戏根目录" : "");
  const knowledgeNoticeMessage = knowledgeError ? knowledgeError : knowledgeStep || (knowledgeNotes[0] ?? "");
  const floatingNoticeCandidates: Array<StatusNoticeItem | null> = [
    activeTab === "workspace" && (saveError || configDirty || Boolean(saveNotice))
      ? {
          id: "workspace-save",
          title: "工作区设置自动保存",
          tone: saveError ? "error" : saving || configDirty ? "info" : "success",
          message: saveError || workspaceSaveStatus,
          indeterminate: saving,
        }
      : null,
    activeTab === "workspace" && (detecting || Boolean(pathNoticeMessage))
      ? {
          id: "path-detect",
          title: detecting
            ? "自动检测路径"
            : pathFailureNote
              ? "路径检测失败"
              : pathSuccessNotes.length > 0
                ? "路径已更新"
                : "项目路径提示",
          tone: pathFailureNote ? "error" : detecting ? "warning" : pathSuccessNotes.length > 0 ? "success" : "warning",
          message: pathNoticeMessage,
          details: detecting ? pathNotes.slice(0, 3) : pathNotes.slice(1, 3),
          indeterminate: detecting,
          actions: detecting ? (
            <button
              type="button"
              onClick={cancelDetectPaths}
              className="rounded-lg border border-rose-200 px-3 py-1.5 text-xs font-medium text-rose-700 transition hover:bg-rose-100"
            >
              中断检测
            </button>
          ) : undefined,
        }
      : null,
    activeTab === "workspace" && (knowledgeChecking || Boolean(knowledgeTaskId) || Boolean(knowledgeNoticeMessage))
      ? {
          id: "knowledge-status",
          title: knowledgeError
            ? "知识库操作失败"
            : knowledgeTaskId
              ? "知识库更新中"
              : knowledgeChecking
                ? "知识库检查中"
                : "知识库状态提示",
          tone: knowledgeError ? "error" : knowledgeTaskId ? "warning" : knowledgeChecking ? "info" : "success",
          message: knowledgeNoticeMessage || "知识库状态已更新",
          details: knowledgeNotes.slice(0, 3),
          progress: knowledgeTaskId ? knowledgeUpdateProgress : undefined,
          indeterminate: knowledgeChecking,
        }
      : null,
    activeTab === "server" && (serverError || serverSaving || serverSelectionDirty || Boolean(serverNotice))
      ? {
          id: "server-save",
          title: "服务器默认配置",
          tone: serverError ? "error" : serverSaving || serverSelectionDirty ? "info" : "success",
          message: serverError || serverStatusText,
          indeterminate: serverSaving,
        }
      : null,
  ];
  const floatingNotices = floatingNoticeCandidates.filter((notice): notice is StatusNoticeItem => notice !== null);

  async function handleCheckKnowledge() {
    if (knowledgeBusy) {
      return;
    }

    setKnowledgeChecking(true);
    setKnowledgeError("");
    setKnowledgeStep("检查知识库状态");
    setKnowledgeNotes([]);
    try {
      const status = await checkKnowledgeStatus();
      if (!mountedRef.current) {
        return;
      }
      setKnowledgeStatus(status);
      onKnowledgeStatusChange?.(status);
      setKnowledgeNotes(status.warnings ?? []);
      setKnowledgeStep("");
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setKnowledgeError(resolveErrorMessage(error));
      setKnowledgeStep("检查失败");
    } finally {
      if (mountedRef.current) {
        setKnowledgeChecking(false);
      }
    }
  }

  async function handleRefreshKnowledge() {
    if (knowledgeBusy) {
      return;
    }

    setKnowledgeError("");
    setKnowledgeNotes([]);
    setKnowledgeStep("准备启动知识库更新任务");
    try {
      const task = await startRefreshKnowledgeTask();
      if (!mountedRef.current) {
        retainedKnowledgeTaskId = task.task_id;
        return;
      }
      setKnowledgeTaskId(task.task_id);
      setKnowledgeStep(task.current_step || "更新中");
      setKnowledgeNotes(task.notes ?? []);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setKnowledgeError(resolveErrorMessage(error));
    }
  }

  async function handleSaveServerPreference(profileId: number | null) {
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

  async function handleCloseSettings() {
    if (!onClose) {
      return;
    }
    try {
      if (configDirty) {
        await save();
      }
      if (serverSelectionDirty) {
        await handleSaveServerPreference(selectedServerProfileId ?? null);
      }
      onClose();
    } catch {
      // 保存函数已经负责展示错误，失败时保留设置面板让用户处理。
    }
  }

  const header = (
    <div
      className={`border-b border-slate-200 px-6 py-4 flex items-center z-10 ${
        mode === "drawer"
          ? "sticky top-0 bg-white justify-between"
          : "justify-between gap-4 bg-slate-50/80 backdrop-blur"
      }`}
    >
      <div>
        <h2 className="font-bold text-slate-800">设置</h2>
      </div>
      {mode === "drawer" && onClose ? (
        <button
          onClick={() => {
            void handleCloseSettings();
          }}
          className="text-slate-400 hover:text-slate-600 transition-colors p-1 rounded-lg hover:bg-slate-100"
        >
          <X size={18} />
        </button>
      ) : null}
    </div>
  );

  const tabBar = (
    <div className="border-b border-slate-100 px-6 py-3">
      <div className="flex flex-wrap gap-2">
        {(
          [
            { key: "workspace", label: "工作站" },
            { key: "server", label: "服务器模式" },
          ] as const
        ).map((tab) => (
          <button
            key={tab.key}
            type="button"
            onClick={() => setActiveTab(tab.key)}
            className={`rounded-full border px-3 py-1.5 text-sm transition-colors ${
              activeTab === tab.key
                ? "border-amber-300 bg-amber-50 text-amber-700"
                : "border-slate-200 text-slate-500 hover:border-amber-200 hover:text-amber-700"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
    </div>
  );

  const content = (
    <div className="p-6 space-y-6">
      {!cfg ? (
        <p className="text-slate-400 text-sm">加载中…</p>
      ) : activeTab === "workspace" ? (
        <>
          {/* ── 项目配置（最重要，放最前面）── */}
          <SGroup icon={<FolderOpen size={14} />} title="项目配置">
            <div className="flex items-center gap-2">
              <button
                onClick={detectPaths}
                disabled={detecting}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-200 text-slate-500 text-xs hover:border-amber-300 hover:text-amber-600 disabled:opacity-40 transition-colors"
              >
                <Search size={12} />
                {detecting ? "检测中…" : "自动检测路径"}
              </button>
              {detecting && (
                <button
                  type="button"
                  onClick={cancelDetectPaths}
                  className="px-3 py-1.5 rounded-lg border border-rose-200 text-rose-600 text-xs hover:bg-rose-50 transition-colors"
                >
                  中断检测
                </button>
              )}
            </div>
            <Field label="默认 Mod 项目目录" hint="新建/修改 Mod 时的默认路径">
              <div className="flex gap-2">
                <input
                  value={cfg.default_project_root || ""}
                  readOnly
                  placeholder="E:/STS2mod/testscenario"
                  className={readonlyInputCls + " font-mono"}
                />
                <button
                  type="button"
                  onClick={() => choosePath("default_project_root")}
                  className="shrink-0 rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 hover:border-amber-300 hover:text-amber-600 transition-colors"
                >
                  选择目录
                </button>
              </div>
            </Field>
            <Field label="STS2 游戏根目录" hint="用于一键部署 Mod 文件">
              <div className="flex gap-2">
                <input
                  value={cfg.sts2_path || ""}
                  readOnly
                  placeholder="E:/steam/steamapps/common/Slay the Spire 2"
                  className={readonlyInputCls + " font-mono"}
                />
                <button
                  type="button"
                  onClick={() => choosePath("sts2_path")}
                  className="shrink-0 rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 hover:border-amber-300 hover:text-amber-600 transition-colors"
                >
                  选择目录
                </button>
              </div>
            </Field>
            <Field label="Godot 4.5.1 Mono 路径" hint="用于打包 .pck 文件，必须是 4.5.1 Mono 版本">
              <div className="flex gap-2">
                <input
                  value={cfg.godot_exe_path || ""}
                  readOnly
                  placeholder="C:/tools/Godot_v4.5.1-stable_mono_win64.exe"
                  className={readonlyInputCls + " font-mono"}
                />
                <button
                  type="button"
                  onClick={() => choosePath("godot_exe_path")}
                  className="shrink-0 rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 hover:border-amber-300 hover:text-amber-600 transition-colors"
                >
                  选择文件
                </button>
              </div>
            </Field>
          </SGroup>

          <div className="border-t border-slate-100" />

          <SGroup icon={<Gamepad2 size={14} />} title="知识库状态">
            <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3 space-y-2">
              <p className="text-xs text-slate-500">
                当前状态：<span className="font-semibold text-slate-700">{knowledgeStatus?.status || "loading"}</span>
              </p>
              <p className="text-xs text-slate-500">
                游戏版本：
                <span className="font-semibold text-slate-700">
                  {knowledgeStatus?.game?.current_version || knowledgeStatus?.game?.version || "未知"}
                </span>
              </p>
              <p className="text-xs text-slate-500">
                游戏知识来源：
                <span className="font-semibold text-slate-700">{knowledgeStatus?.game?.source_mode || "未知"}</span>
              </p>
              <p className="text-xs text-slate-500">
                Baselib release：
                <span className="font-semibold text-slate-700">
                  {knowledgeStatus?.baselib?.latest_release_tag || knowledgeStatus?.baselib?.release_tag || "未知"}
                </span>
              </p>
              <p className="text-xs text-slate-500">
                Baselib 知识来源：
                <span className="font-semibold text-slate-700">{knowledgeStatus?.baselib?.source_mode || "未知"}</span>
              </p>
              {knowledgeChecking ? (
                <ProgressBar label="检查进度" tone="sky" indeterminate progress={knowledgeCheckProgress} />
              ) : null}
              {knowledgeTaskId ? <ProgressBar label="更新进度" progress={knowledgeUpdateProgress} /> : null}
            </div>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => {
                  void handleCheckKnowledge();
                }}
                disabled={knowledgeBusy}
                className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 hover:border-amber-300 hover:text-amber-700 transition-colors"
              >
                {knowledgeChecking ? "检查中…" : "检查更新"}
              </button>
              <button
                type="button"
                onClick={() => {
                  void handleRefreshKnowledge();
                }}
                disabled={knowledgeBusy}
                className="rounded-lg border border-amber-300 px-3 py-1.5 text-xs text-amber-700 hover:bg-amber-50 disabled:opacity-50 transition-colors"
              >
                {knowledgeBusy ? "更新中…" : "更新知识库"}
              </button>
              <button
                type="button"
                onClick={() => setKnowledgeGuideOpen(true)}
                className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 hover:border-amber-300 hover:text-amber-700 transition-colors"
              >
                查看知识库说明
              </button>
            </div>
          </SGroup>

          <div className="border-t border-slate-100" />

          {/* ── LLM 配置 ── */}
          <SGroup icon={<Cpu size={14} />} title="LLM 配置">
            <Field label="文本任务模式" hint="规划、日志分析、提示词优化以及代码代理都会跟随这里的模式。">
              <select
                value={cfg.llm?.mode || ""}
                onChange={(e) => set(["llm", "mode"], e.target.value)}
                className={selectCls}
              >
                <option value="agent_cli">Agent CLI</option>
                <option value="claude_api">Claude API</option>
              </select>
            </Field>
            {cfg.llm?.mode === "agent_cli" ? (
              <Field label="代码代理后端" hint="CLI 模式下，文本任务和代码代理都会走这里选择的后端。">
                <select
                  value={cfg.llm?.agent_backend || "claude"}
                  onChange={(e) => set(["llm", "agent_backend"], e.target.value)}
                  className={selectCls}
                >
                  <option value="claude">Claude CLI</option>
                  <option value="codex">Codex CLI</option>
                </select>
              </Field>
            ) : (
              <Field
                label="代码代理后端"
                hint="Claude API 模式下，文本任务和代码代理都会直接使用同一套 Claude API 配置。"
              >
                <input value="自动选择：Claude API" readOnly className={readonlyInputCls} />
              </Field>
            )}
            <Field
              label="代码执行模式"
              hint="审批模式：执行前展示操作预览，用户确认后再调用代理。推荐在使用 Codex 时开启。"
            >
              <select
                value={cfg.llm?.execution_mode || "direct_execute"}
                onChange={(e) => set(["llm", "execution_mode"], e.target.value)}
                className={selectCls}
              >
                <option value="direct_execute">直接执行</option>
                <option value="approval_first">审批后执行</option>
              </select>
            </Field>
            {cfg.llm?.mode === "claude_api" && (
              <Field label="Claude 模型" hint="文本任务和代码代理统一使用这个 Claude 模型；必填。">
                <input
                  value={cfg.llm?.model || ""}
                  onChange={(e) => set(["llm", "model"], e.target.value)}
                  placeholder="例如 claude-sonnet-4-6"
                  className={inputCls}
                />
              </Field>
            )}
            {cfg.llm?.mode === "agent_cli" && (
              <Field label="CLI 模型（可选）" hint="留空使用 Codex 或 Claude CLI 的默认模型">
                <input
                  value={cfg.llm?.model || ""}
                  onChange={(e) => set(["llm", "model"], e.target.value)}
                  placeholder="例如 gpt-5-codex / opus / sonnet"
                  className={inputCls}
                />
              </Field>
            )}
            <Field label={cfg.llm?.mode === "claude_api" ? "Claude API Key（留空不修改）" : "API Key（留空不修改）"}>
              <input
                value={llmKey}
                onChange={(e) => {
                  setSaveError("");
                  setSaveNotice("");
                  setLlmKey(e.target.value);
                }}
                placeholder={cfg.llm?.api_key ? "已设置" : "未设置"}
                className={inputCls}
              />
            </Field>
            <Field label={cfg.llm?.mode === "claude_api" ? "Claude Base URL（可选）" : "Base URL（可选）"}>
              <input
                value={cfg.llm?.base_url || ""}
                onChange={(e) => set(["llm", "base_url"], e.target.value)}
                placeholder="https://..."
                className={inputCls}
              />
            </Field>
            <Field label="AI 附加提示词" hint="会追加到全部 AI 调用，包括文本分析、规划、提示词优化和代码代理">
              <textarea
                value={cfg.llm?.custom_prompt || ""}
                onChange={(e) => set(["llm", "custom_prompt"], e.target.value)}
                placeholder="例如：始终用简体中文回答；优先最小改动；输出先给结论后给细节"
                rows={5}
                className={inputCls + " min-h-28 resize-y"}
              />
            </Field>
          </SGroup>

          <div className="border-t border-slate-100" />

          {/* ── 图像生成 ── */}
          <SGroup icon={<Image size={14} />} title="图像生成">
            <Field label="提供商">
              <select
                value={currentProvider}
                onChange={(e) => handleProviderChange(e.target.value)}
                className={selectCls}
              >
                <option value="bfl">BFL (FLUX.2)</option>
                <option value="fal">fal.ai</option>
                <option value="volcengine">火山引擎 (即梦 Seedream)</option>
                <option value="wanxiang">通义万相</option>
              </select>
            </Field>
            {models.length > 0 && (
              <Field label="模型">
                <select
                  value={cfg.image_gen?.model || ""}
                  onChange={(e) => set(["image_gen", "model"], e.target.value)}
                  className={selectCls}
                >
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </Field>
            )}
            {currentProvider === "volcengine" ? (
              <>
                <Field label="Access Key（AK，留空不修改）">
                  <input
                    value={imgKey}
                    onChange={(e) => {
                      setSaveError("");
                      setSaveNotice("");
                      setImgKey(e.target.value);
                    }}
                    placeholder={cfg.image_gen?.api_key ? "已设置" : "未设置"}
                    className={inputCls}
                  />
                </Field>
                <Field label="Secret Key（SK，留空不修改）">
                  <input
                    type="password"
                    value={imgSecret}
                    onChange={(e) => {
                      setSaveError("");
                      setSaveNotice("");
                      setImgSecret(e.target.value);
                    }}
                    placeholder={cfg.image_gen?.api_secret ? "已设置" : "未设置"}
                    className={inputCls}
                  />
                </Field>
              </>
            ) : (
              <Field label="API Key（留空不修改）">
                <input
                  value={imgKey}
                  onChange={(e) => {
                    setSaveError("");
                    setSaveNotice("");
                    setImgKey(e.target.value);
                  }}
                  placeholder={cfg.image_gen?.api_key ? "已设置" : "未设置"}
                  className={inputCls}
                />
              </Field>
            )}
            <Field
              label="背景去除模型"
              hint="birefnet-general 质量最高但慢；birefnet-lite 快一倍；u2net 最快（适合 CPU）"
            >
              <select
                value={cfg.image_gen?.rembg_model || "birefnet-general"}
                onChange={(e) => set(["image_gen", "rembg_model"], e.target.value)}
                className={selectCls}
              >
                <option value="birefnet-general">birefnet-general（最高质量）</option>
                <option value="birefnet-general-lite">birefnet-general-lite（质量/速度均衡）</option>
                <option value="isnet-general-use">isnet-general-use（通用）</option>
                <option value="u2net">u2net（最快，适合无 GPU）</option>
              </select>
            </Field>
            <Field label="并发生图数量" hint="1=串行（推荐，避免 API 限流）；提高可加速但易触发并发限制">
              <input
                type="number"
                min={1}
                max={4}
                value={cfg.image_gen?.concurrency ?? 1}
                onChange={(e) => set(["image_gen", "concurrency"], parseInt(e.target.value) || 1)}
                className={inputCls}
              />
            </Field>
            <div className="flex justify-end">
              <button
                type="button"
                onClick={() => void handleTestImageGenerationConfig()}
                disabled={imageTestLoading}
                className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-600 transition-colors hover:border-amber-300 hover:text-amber-700 disabled:cursor-not-allowed disabled:opacity-60"
              >
                {imageTestLoading ? "测试中…" : "测试生图配置"}
              </button>
            </div>
          </SGroup>
        </>
      ) : (
        <div className="space-y-6">
          <SGroup icon={<Cloud size={14} />} title="服务器模式">
            {!isAuthAvailable ? (
              <StatusNotice
                title="服务器模式暂不可用"
                tone="warning"
                message="当前环境未接入独立 Web 平台服务，暂时无法管理服务器模式默认配置。"
              />
            ) : !isAuthenticated ? (
              <StatusNotice
                title="登录后可管理服务器模式"
                tone="warning"
                message="登录后即可查看平台提供的执行配置，并设置默认服务器模式。"
                actions={
                  <Link
                    to="/auth/login"
                    className="inline-flex rounded-lg border border-amber-300 px-3 py-1.5 text-xs font-medium text-amber-700 transition hover:bg-amber-100"
                  >
                    去登录
                  </Link>
                }
              />
            ) : serverLoading ? (
              <p className="text-sm text-slate-500">正在读取服务器执行配置…</p>
            ) : (
              <>
                {serverPreference?.default_execution_profile_id && !serverPreference.available ? (
                  <StatusNotice
                    title="默认服务器配置已不可用"
                    tone="warning"
                    message="你可以改选一个可用配置，系统会自动保存；也可以直接清空默认值。"
                  />
                ) : null}
                <div className="rounded-xl border border-slate-200 bg-slate-50 px-4 py-3 space-y-2">
                  <p className="text-xs text-slate-500">
                    当前默认配置：
                    <span className="ml-1 font-semibold text-slate-700">
                      {serverPreference?.default_execution_profile_id
                        ? serverPreference.display_name ||
                          `${serverPreference.agent_backend} / ${serverPreference.model}`
                        : "未设置"}
                    </span>
                  </p>
                  <p className="text-xs text-slate-500">
                    可用状态：
                    <span
                      className={`ml-1 font-semibold ${(serverPreference?.available ?? true) ? "text-emerald-700" : "text-amber-700"}`}
                    >
                      {serverPreference?.default_execution_profile_id
                        ? serverPreference?.available
                          ? "当前可用"
                          : "当前默认值已不可用"
                        : "将按运行时选择或后端兜底决定"}
                    </span>
                  </p>
                  <p className="text-xs text-slate-500">
                    运行时执行弹窗会复用这里的默认值，但仍可临时改用其他服务器配置。
                  </p>
                </div>

                <div className="space-y-2">
                  {serverProfiles.map((profile) => {
                    const selected = selectedServerProfileId === profile.id;
                    const disabled = serverSaving || !profile.available;
                    return (
                      <button
                        key={profile.id}
                        type="button"
                        onClick={() => {
                          if (!profile.available) {
                            return;
                          }
                          setSelectedServerProfileId(profile.id);
                          setServerSelectionDirty(true);
                          setServerError("");
                          setServerNotice("");
                        }}
                        disabled={disabled}
                        className={`w-full rounded-xl border px-4 py-3 text-left transition ${
                          selected
                            ? "border-amber-300 bg-amber-50"
                            : "border-slate-200 hover:border-amber-200 hover:bg-amber-50/40"
                        } ${disabled ? "cursor-not-allowed opacity-60 hover:border-slate-200 hover:bg-white" : ""}`}
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div>
                            <p className="text-sm font-medium text-slate-900">{profile.display_name}</p>
                            <p className="mt-1 text-xs text-slate-500">{profile.description}</p>
                          </div>
                          <div className="shrink-0 space-y-1 text-right">
                            {profile.recommended ? (
                              <p className="text-[11px] font-medium text-emerald-700">推荐</p>
                            ) : null}
                            <p
                              className={`text-[11px] font-medium ${profile.available ? "text-slate-500" : "text-amber-700"}`}
                            >
                              {profile.available ? "可用" : "当前不可用"}
                            </p>
                          </div>
                        </div>
                      </button>
                    );
                  })}
                </div>

                <div className="flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedServerProfileId(null);
                      setServerSelectionDirty(true);
                      setServerError("");
                      setServerNotice("");
                    }}
                    disabled={serverSaving || serverPreference?.default_execution_profile_id === null}
                    className="rounded-lg border border-slate-200 px-4 py-2 text-sm text-slate-600 transition hover:border-amber-200 hover:text-amber-700 disabled:opacity-40"
                  >
                    清空默认配置
                  </button>
                </div>
              </>
            )}
          </SGroup>
        </div>
      )}
    </div>
  );

  const guideDialog = (
    <KnowledgeGuideDialog
      open={knowledgeGuideOpen}
      status={knowledgeStatus}
      onClose={() => setKnowledgeGuideOpen(false)}
    />
  );

  const progressAnimationStyle = (
    <style>{`
      @keyframes knowledge-progress-indeterminate {
        0% { transform: translateX(-120%); }
        100% { transform: translateX(320%); }
      }
    `}</style>
  );

  if (mode === "page") {
    return (
      <>
        <StatusNoticeStack notices={floatingNotices} />
        <section className="overflow-hidden rounded-[28px] border border-slate-200 bg-white shadow-[0_30px_80px_rgba(15,23,42,0.08)]">
          {header}
          {tabBar}
          {content}
        </section>
        {progressAnimationStyle}
        {guideDialog}
      </>
    );
  }

  return (
    <>
      <StatusNoticeStack notices={floatingNotices} />
      <div
        className="fixed inset-0 bg-black/60 z-50 flex justify-end"
        onClick={() => {
          void handleCloseSettings();
        }}
      >
        <div
          className="w-full max-w-sm bg-white border-l border-slate-200 h-full overflow-y-auto shadow-xl"
          onClick={(e) => e.stopPropagation()}
        >
          {header}
          {tabBar}
          {content}
        </div>
      </div>
      {progressAnimationStyle}
      {guideDialog}
    </>
  );
}
