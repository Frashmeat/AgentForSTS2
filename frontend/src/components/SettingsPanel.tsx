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
  loadPlatformQueueWorkerStatus,
  pickAppPath,
  startDetectAppPaths,
  startRefreshKnowledgeTask,
  testImageGenerationConfig,
  type KnowledgeStatus,
  type MyServerPreferenceView,
  type PlatformQueueWorkerStatus,
  updateAppConfig,
  updateMyServerPreferences,
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";
import type { PlatformExecutionProfile } from "../shared/api/platform.ts";
import { useSession } from "../shared/session/hooks.ts";
import { KnowledgeGuideDialog } from "./KnowledgeGuideDialog.tsx";
import { StatusNotice, StatusNoticeStack, type StatusNoticeItem } from "./StatusNotice.tsx";
import { createSettingsPickPathRequest, type SettingsPathField } from "./settingsPathPicker.ts";

const inputCls = "w-full bg-white border border-slate-200 rounded-lg px-3 py-1.5 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-amber-400 focus:ring-1 focus:ring-amber-100";
const selectCls = "w-full bg-white border border-slate-200 rounded-lg px-3 py-1.5 text-sm text-slate-800 focus:outline-none focus:border-amber-400";
const readonlyInputCls = `${inputCls} bg-slate-50 text-slate-500`;

const PROVIDER_MODELS: Record<string, string[]> = {
  bfl:         ["flux.2-flex", "flux.2-pro", "flux.2-klein", "flux.2-max", "flux.1.1-pro"],
  fal:         ["flux.2-flex", "flux.2-pro", "flux.2-dev", "flux.2-schnell"],
  volcengine:  ["doubao-seedream-3-0-t2i-250415", "doubao-seedream-3-0-1-5b-t2i-250616"],
  wanxiang:    [],
};

const KNOWLEDGE_UPDATE_STEP_PROGRESS: Array<{ pattern: RegExp; progress: number }> = [
  { pattern: /初始化更新任务|准备启动知识库更新任务|等待开始/, progress: 8 },
  { pattern: /读取当前游戏版本/, progress: 18 },
  { pattern: /反编译游戏源码/, progress: 48 },
  { pattern: /读取 Baselib latest release/, progress: 62 },
  { pattern: /下载 Baselib\.dll/, progress: 78 },
  { pattern: /反编译 Baselib/, progress: 92 },
  { pattern: /更新完成/, progress: 100 },
  { pattern: /更新失败/, progress: 100 },
];

function getKnowledgeUpdateProgress(step: string, busy: boolean) {
  if (!step.trim()) {
    return busy ? 8 : 0;
  }

  for (const item of KNOWLEDGE_UPDATE_STEP_PROGRESS) {
    if (item.pattern.test(step)) {
      return item.progress;
    }
  }

  return busy ? 24 : 100;
}

function ProgressBar({
  label,
  progress,
  tone = "amber",
  indeterminate = false,
}: {
  label: string;
  progress?: number;
  tone?: "amber" | "sky";
  indeterminate?: boolean;
}) {
  const trackCls = tone === "sky" ? "bg-sky-100" : "bg-amber-100";
  const fillCls = tone === "sky" ? "bg-sky-500" : "bg-amber-500";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-3">
        <p className="text-[11px] font-medium text-slate-600">{label}</p>
        {!indeterminate && typeof progress === "number" ? (
          <span className="text-[11px] font-semibold text-slate-500">{Math.round(progress)}%</span>
        ) : null}
      </div>
      <div className={`h-2 overflow-hidden rounded-full ${trackCls}`}>
        {indeterminate ? (
          <div className="h-full w-1/3 rounded-full bg-sky-500 animate-[knowledge-progress-indeterminate_1.2s_ease-in-out_infinite]" />
        ) : (
          <div
            className={`h-full rounded-full ${fillCls} transition-[width] duration-300 ease-out`}
            style={{ width: `${Math.max(0, Math.min(progress ?? 0, 100))}%` }}
          />
        )}
      </div>
    </div>
  );
}

function SGroup({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-slate-400">{icon}</span>
        <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider">{title}</h3>
      </div>
      <div className="space-y-2.5 pl-1">{children}</div>
    </div>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <label className="text-xs font-medium text-slate-500">{label}</label>
      {children}
      {hint && <p className="text-xs text-slate-400">{hint}</p>}
    </div>
  );
}

function pickInitialServerProfileId(
  profiles: PlatformExecutionProfile[],
  preference: MyServerPreferenceView | null,
): number | null {
  if (preference?.default_execution_profile_id !== null && typeof preference?.default_execution_profile_id !== "undefined") {
    const preferredProfile = profiles.find((profile) => profile.id === preference.default_execution_profile_id);
    return preferredProfile?.available ? preferredProfile.id : null;
  }
  return profiles.find((profile) => profile.available && profile.recommended)?.id
    ?? profiles.find((profile) => profile.available)?.id
    ?? null;
}

function formatDiagnosticTime(value?: string | null): string {
  const text = String(value ?? "").trim();
  if (!text) {
    return "—";
  }
  const date = new Date(text);
  if (Number.isNaN(date.getTime())) {
    return text;
  }
  return date.toLocaleString("zh-CN", { hour12: false });
}

function formatQueueWorkerUnavailableReason(reason?: string): string {
  switch (String(reason ?? "").trim()) {
    case "app_state_missing":
      return "当前窗口未挂上应用状态，无法读取 queue worker。";
    case "queue_worker_not_registered":
      return "当前运行角色未注册平台 queue worker。";
    default:
      return "当前未暴露 queue worker 运行态。";
  }
}

function formatLeaderEventLabel(eventType?: string): string {
  switch (String(eventType ?? "").trim()) {
    case "leader_acquired":
      return "成为 Leader";
    case "leader_taken_over":
      return "接管 Leader";
    case "leader_observed_other":
      return "观察到其他 Leader";
    case "leader_lost":
      return "失去 Leader";
    case "leader_released":
      return "主动释放 Leader";
    case "leader_waiting_for_failover":
      return "等待 Failover";
    default:
      return String(eventType ?? "").trim() || "未知事件";
  }
}

function getLeaderEventTone(eventType?: string): "default" | "good" | "warn" {
  switch (String(eventType ?? "").trim()) {
    case "leader_acquired":
    case "leader_taken_over":
      return "good";
    case "leader_lost":
    case "leader_waiting_for_failover":
    case "leader_observed_other":
      return "warn";
    default:
      return "default";
  }
}

function DiagnosticField({
  label,
  value,
  tone = "default",
  breakAll = false,
}: {
  label: string;
  value: React.ReactNode;
  tone?: "default" | "good" | "warn";
  breakAll?: boolean;
}) {
  const valueCls =
    tone === "good"
      ? "text-emerald-700"
      : tone === "warn"
        ? "text-amber-700"
        : "text-slate-700";
  return (
    <p className="text-xs text-slate-500">
      {label}：
      <span className={`ml-1 font-semibold ${valueCls} ${breakAll ? "break-all" : ""}`}>{value}</span>
    </p>
  );
}

function DiagnosticCard({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-xl border border-slate-200 bg-white px-4 py-3 space-y-2">
      <p className="text-xs font-semibold uppercase tracking-wider text-slate-500">{title}</p>
      {children}
    </div>
  );
}

interface SettingsPanelProps {
  mode?: "drawer" | "page";
  onClose?: () => void;
  onKnowledgeStatusChange?: (status: KnowledgeStatus) => void;
}

function buildConfigSaveBody(cfg: any, llmKey: string, imgKey: string, imgSecret: string) {
  const body = structuredClone(cfg);
  if (llmKey.trim()) body.llm.api_key = llmKey.trim();
  if (imgKey.trim()) body.image_gen.api_key = imgKey.trim();
  if (imgSecret.trim()) body.image_gen.api_secret = imgSecret.trim();
  return body;
}

function buildConfigSaveSignature(cfg: any, llmKey: string, imgKey: string, imgSecret: string) {
  return JSON.stringify(buildConfigSaveBody(cfg, llmKey, imgKey, imgSecret));
}

let retainedDetectionTaskId = "";
let retainedKnowledgeTaskId = "";

export function SettingsPanel({ mode = "drawer", onClose, onKnowledgeStatusChange }: SettingsPanelProps) {
  const { isAuthAvailable, isAuthenticated } = useSession();
  const [activeTab, setActiveTab] = useState<"workspace" | "server">("workspace");
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
  const [queueWorkerStatus, setQueueWorkerStatus] = useState<PlatformQueueWorkerStatus | null>(null);
  const [queueWorkerLoading, setQueueWorkerLoading] = useState(false);
  const [queueWorkerError, setQueueWorkerError] = useState("");
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
  }, [activeTab, isAuthAvailable, isAuthenticated, selectedServerProfileId, serverSaving, serverSelectionDirty]);

  useEffect(() => {
    if (activeTab !== "server") {
      return;
    }

    let cancelled = false;
    setQueueWorkerLoading(true);
    setQueueWorkerError("");

    void loadPlatformQueueWorkerStatus()
      .then((status) => {
        if (!cancelled) {
          setQueueWorkerStatus(status);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setQueueWorkerStatus(null);
          setQueueWorkerError(resolveErrorMessage(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setQueueWorkerLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [activeTab]);

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
          setPathNotes(prev => [...(snapshot.notes ?? prev), `检测失败：${message}`]);
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
  const knowledgeNoticeMessage = knowledgeError
    ? knowledgeError
    : knowledgeStep || (knowledgeNotes[0] ?? "");
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
          title: detecting ? "自动检测路径" : pathFailureNote ? "路径检测失败" : pathSuccessNotes.length > 0 ? "路径已更新" : "项目路径提示",
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
          title: knowledgeError ? "知识库操作失败" : knowledgeTaskId ? "知识库更新中" : knowledgeChecking ? "知识库检查中" : "知识库状态提示",
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

  async function refreshQueueWorkerStatus() {
    setQueueWorkerLoading(true);
    setQueueWorkerError("");
    try {
      const status = await loadPlatformQueueWorkerStatus();
      if (!mountedRef.current) {
        return;
      }
      setQueueWorkerStatus(status);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setQueueWorkerStatus(null);
      setQueueWorkerError(resolveErrorMessage(error));
    } finally {
      if (mountedRef.current) {
        setQueueWorkerLoading(false);
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
        <button onClick={() => { void handleCloseSettings(); }} className="text-slate-400 hover:text-slate-600 transition-colors p-1 rounded-lg hover:bg-slate-100">
          <X size={18} />
        </button>
      ) : null}
    </div>
  );

  const tabBar = (
    <div className="border-b border-slate-100 px-6 py-3">
      <div className="flex flex-wrap gap-2">
        {([
          { key: "workspace", label: "工作站" },
          { key: "server", label: "服务器模式" },
        ] as const).map((tab) => (
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
      ) : (
        activeTab === "workspace" ? (
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
                <Field
                  label="默认 Mod 项目目录"
                  hint="新建/修改 Mod 时的默认路径"
                >
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
                <Field
                  label="STS2 游戏根目录"
                  hint="用于一键部署 Mod 文件"
                >
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
                <Field
                  label="Godot 4.5.1 Mono 路径"
                  hint="用于打包 .pck 文件，必须是 4.5.1 Mono 版本"
                >
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
                  <p className="text-xs text-slate-500">当前状态：<span className="font-semibold text-slate-700">{knowledgeStatus?.status || "loading"}</span></p>
                  <p className="text-xs text-slate-500">游戏版本：<span className="font-semibold text-slate-700">{knowledgeStatus?.game?.current_version || knowledgeStatus?.game?.version || "未知"}</span></p>
                  <p className="text-xs text-slate-500">游戏知识来源：<span className="font-semibold text-slate-700">{knowledgeStatus?.game?.source_mode || "未知"}</span></p>
                  <p className="text-xs text-slate-500">Baselib release：<span className="font-semibold text-slate-700">{knowledgeStatus?.baselib?.latest_release_tag || knowledgeStatus?.baselib?.release_tag || "未知"}</span></p>
                  <p className="text-xs text-slate-500">Baselib 知识来源：<span className="font-semibold text-slate-700">{knowledgeStatus?.baselib?.source_mode || "未知"}</span></p>
                  {knowledgeChecking ? (
                    <ProgressBar label="检查进度" tone="sky" indeterminate progress={knowledgeCheckProgress} />
                  ) : null}
                  {knowledgeTaskId ? (
                    <ProgressBar label="更新进度" progress={knowledgeUpdateProgress} />
                  ) : null}
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
                  <select value={cfg.llm?.mode || ""} onChange={e => set(["llm", "mode"], e.target.value)} className={selectCls}>
                    <option value="agent_cli">Agent CLI</option>
                    <option value="claude_api">Claude API</option>
                  </select>
                </Field>
                {cfg.llm?.mode === "agent_cli" ? (
                  <Field label="代码代理后端" hint="CLI 模式下，文本任务和代码代理都会走这里选择的后端。">
                    <select
                      value={cfg.llm?.agent_backend || "claude"}
                      onChange={e => set(["llm", "agent_backend"], e.target.value)}
                      className={selectCls}
                    >
                      <option value="claude">Claude CLI</option>
                      <option value="codex">Codex CLI</option>
                    </select>
                  </Field>
                ) : (
                  <Field label="代码代理后端" hint="Claude API 模式下，文本任务和代码代理都会直接使用同一套 Claude API 配置。">
                    <input
                      value="自动选择：Claude API"
                      readOnly
                      className={readonlyInputCls}
                    />
                  </Field>
                )}
                <Field
                  label="代码执行模式"
                  hint="审批模式：执行前展示操作预览，用户确认后再调用代理。推荐在使用 Codex 时开启。"
                >
                  <select
                    value={cfg.llm?.execution_mode || "direct_execute"}
                    onChange={e => set(["llm", "execution_mode"], e.target.value)}
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
                      onChange={e => set(["llm", "model"], e.target.value)}
                      placeholder="例如 claude-sonnet-4-6"
                      className={inputCls}
                    />
                  </Field>
                )}
                {cfg.llm?.mode === "agent_cli" && (
                  <Field label="CLI 模型（可选）" hint="留空使用 Codex 或 Claude CLI 的默认模型">
                    <input
                      value={cfg.llm?.model || ""}
                      onChange={e => set(["llm", "model"], e.target.value)}
                      placeholder="例如 gpt-5-codex / opus / sonnet"
                      className={inputCls}
                    />
                  </Field>
                )}
                <Field label={cfg.llm?.mode === "claude_api" ? "Claude API Key（留空不修改）" : "API Key（留空不修改）"}>
                  <input
                    value={llmKey}
                    onChange={e => {
                      setSaveError("");
                      setSaveNotice("");
                      setLlmKey(e.target.value);
                    }}
                    placeholder={cfg.llm?.api_key ? "已设置" : "未设置"}
                    className={inputCls}
                  />
                </Field>
                <Field label={cfg.llm?.mode === "claude_api" ? "Claude Base URL（可选）" : "Base URL（可选）"}>
                  <input value={cfg.llm?.base_url || ""} onChange={e => set(["llm", "base_url"], e.target.value)} placeholder="https://..." className={inputCls} />
                </Field>
                <Field label="AI 附加提示词" hint="会追加到全部 AI 调用，包括文本分析、规划、提示词优化和代码代理">
                  <textarea
                    value={cfg.llm?.custom_prompt || ""}
                    onChange={e => set(["llm", "custom_prompt"], e.target.value)}
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
                  <select value={currentProvider} onChange={e => handleProviderChange(e.target.value)} className={selectCls}>
                    <option value="bfl">BFL (FLUX.2)</option>
                    <option value="fal">fal.ai</option>
                    <option value="volcengine">火山引擎 (即梦 Seedream)</option>
                    <option value="wanxiang">通义万相</option>
                  </select>
                </Field>
                {models.length > 0 && (
                  <Field label="模型">
                    <select value={cfg.image_gen?.model || ""} onChange={e => set(["image_gen", "model"], e.target.value)} className={selectCls}>
                      {models.map(m => <option key={m} value={m}>{m}</option>)}
                    </select>
                  </Field>
                )}
                {currentProvider === "volcengine" ? (
                  <>
                    <Field label="Access Key（AK，留空不修改）">
                      <input
                        value={imgKey}
                        onChange={e => {
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
                        onChange={e => {
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
                      onChange={e => {
                        setSaveError("");
                        setSaveNotice("");
                        setImgKey(e.target.value);
                      }}
                      placeholder={cfg.image_gen?.api_key ? "已设置" : "未设置"}
                      className={inputCls}
                    />
                  </Field>
                )}
                <Field label="背景去除模型" hint="birefnet-general 质量最高但慢；birefnet-lite 快一倍；u2net 最快（适合 CPU）">
                  <select
                    value={cfg.image_gen?.rembg_model || "birefnet-general"}
                    onChange={e => set(["image_gen", "rembg_model"], e.target.value)}
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
                    type="number" min={1} max={4}
                    value={cfg.image_gen?.concurrency ?? 1}
                    onChange={e => set(["image_gen", "concurrency"], parseInt(e.target.value) || 1)}
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
                          ? serverPreference.display_name || `${serverPreference.agent_backend} / ${serverPreference.model}`
                          : "未设置"}
                      </span>
                    </p>
                    <p className="text-xs text-slate-500">
                      可用状态：
                      <span className={`ml-1 font-semibold ${serverPreference?.available ?? true ? "text-emerald-700" : "text-amber-700"}`}>
                        {serverPreference?.default_execution_profile_id
                          ? (serverPreference?.available ? "当前可用" : "当前默认值已不可用")
                          : "将按运行时选择或后端兜底决定"}
                      </span>
                    </p>
                    <p className="text-xs text-slate-500">运行时执行弹窗会复用这里的默认值，但仍可临时改用其他服务器配置。</p>
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
                          } ${disabled ? "cursor-not-allowed opacity-60 hover:border-slate-200 hover:bg-white" : ""
                          }`}
                        >
                          <div className="flex items-start justify-between gap-3">
                            <div>
                              <p className="text-sm font-medium text-slate-900">{profile.display_name}</p>
                              <p className="mt-1 text-xs text-slate-500">{profile.description}</p>
                            </div>
                            <div className="shrink-0 space-y-1 text-right">
                              {profile.recommended ? <p className="text-[11px] font-medium text-emerald-700">推荐</p> : null}
                              <p className={`text-[11px] font-medium ${profile.available ? "text-slate-500" : "text-amber-700"}`}>
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
                    <Link
                      to="/admin/server-credentials"
                      className="rounded-lg border border-slate-200 px-4 py-2 text-sm text-slate-600 transition hover:border-amber-200 hover:text-amber-700"
                    >
                      服务器凭据管理
                    </Link>
                  </div>
                </>
              )}
            </SGroup>

            <SGroup icon={<Search size={14} />} title="平台队列 Worker 诊断">
              {queueWorkerLoading ? (
                <p className="text-sm text-slate-500">正在读取 queue worker 状态…</p>
              ) : queueWorkerError ? (
                <StatusNotice title="平台队列 Worker 诊断失败" tone="error" message={queueWorkerError} />
              ) : !queueWorkerStatus?.available ? (
                <StatusNotice title="当前未暴露队列 Worker 运行态" tone="warning" message={formatQueueWorkerUnavailableReason(queueWorkerStatus?.reason)} />
              ) : (
                <div className="space-y-3">
                  <DiagnosticCard title="Leader 概览">
                    <div className="grid gap-2 sm:grid-cols-2">
                      <DiagnosticField label="当前实例" value={queueWorkerStatus.owner_id || "—"} breakAll />
                      <DiagnosticField label="当前角色" value={queueWorkerStatus.owner_scope || "—"} />
                      <DiagnosticField
                        label="Leader 状态"
                        value={queueWorkerStatus.is_leader ? "当前实例是 leader" : "当前实例不是 leader"}
                        tone={queueWorkerStatus.is_leader ? "good" : "default"}
                      />
                      <DiagnosticField label="Leader Epoch" value={queueWorkerStatus.leader_epoch ?? "—"} />
                      <DiagnosticField label="最近 Tick" value={queueWorkerStatus.last_tick_reason || "—"} />
                      <DiagnosticField label="最近 Tick 时间" value={formatDiagnosticTime(queueWorkerStatus.last_tick_at)} />
                      <DiagnosticField label="最近获得 Leader" value={formatDiagnosticTime(queueWorkerStatus.last_leader_acquired_at)} />
                      <DiagnosticField label="最近失去 Leader" value={formatDiagnosticTime(queueWorkerStatus.last_leader_lost_at)} />
                    </div>
                  </DiagnosticCard>

                  <DiagnosticCard title="切换窗口">
                    <div className="grid gap-2 sm:grid-cols-2">
                      <DiagnosticField
                        label="Failover 窗口"
                        value={typeof queueWorkerStatus.failover_window_seconds === "number" ? `${queueWorkerStatus.failover_window_seconds}s` : "—"}
                      />
                      <DiagnosticField
                        label="Retry Grace"
                        value={typeof queueWorkerStatus.leader_retry_grace_seconds === "number" ? `${queueWorkerStatus.leader_retry_grace_seconds}s` : "—"}
                      />
                      <DiagnosticField
                        label="下次重试不早于"
                        value={formatDiagnosticTime(queueWorkerStatus.next_leader_retry_not_before)}
                        tone={queueWorkerStatus.next_leader_retry_not_before ? "warn" : "default"}
                      />
                    </div>
                    <div className="border-t border-slate-100 pt-2">
                      <p className="text-[11px] leading-5 text-slate-500">
                        当前实例如果不是 leader，会根据 leader lease 的过期时间进入短退避窗口，避免每个 tick 都去竞争租约。
                      </p>
                    </div>
                  </DiagnosticCard>

                  <DiagnosticCard title="当前有效 Leader">
                    {queueWorkerStatus.current_leader ? (
                      <div className="grid gap-2 sm:grid-cols-2">
                        <DiagnosticField label="Owner" value={queueWorkerStatus.current_leader.owner_id} breakAll />
                        <DiagnosticField label="Epoch" value={queueWorkerStatus.current_leader.leader_epoch ?? "—"} />
                        <DiagnosticField label="Claimed At" value={formatDiagnosticTime(queueWorkerStatus.current_leader.claimed_at)} />
                        <DiagnosticField label="Renewed At" value={formatDiagnosticTime(queueWorkerStatus.current_leader.renewed_at)} />
                        <div className="sm:col-span-2">
                          <DiagnosticField label="Expires At" value={formatDiagnosticTime(queueWorkerStatus.current_leader.expires_at)} />
                        </div>
                      </div>
                    ) : (
                      <p className="text-sm text-slate-500">当前没有读取到有效的 leader lease。</p>
                    )}
                  </DiagnosticCard>

                  <div className="rounded-xl border border-slate-200 bg-white px-4 py-3 space-y-2">
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-xs font-semibold uppercase tracking-wider text-slate-500">最近 Leader 事件</p>
                      <div className="flex flex-wrap gap-2">
                        <Link
                          to="/admin/runtime-audit"
                          className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-amber-200 hover:text-amber-700"
                        >
                          打开管理员审计页
                        </Link>
                        <button
                          type="button"
                          onClick={() => {
                            void refreshQueueWorkerStatus();
                          }}
                          className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs text-slate-600 transition hover:border-amber-200 hover:text-amber-700"
                        >
                          刷新诊断
                        </button>
                      </div>
                    </div>
                    {queueWorkerStatus.recent_leader_events?.length ? (
                      <div className="space-y-2">
                        {[...queueWorkerStatus.recent_leader_events]
                          .sort((left, right) => {
                            const leftTime = new Date(left.occurred_at).getTime();
                            const rightTime = new Date(right.occurred_at).getTime();
                            return rightTime - leftTime;
                          })
                          .map((event, index) => {
                            const tone = getLeaderEventTone(event.event_type);
                            const titleCls =
                              tone === "good"
                                ? "text-emerald-700"
                                : tone === "warn"
                                  ? "text-amber-700"
                                  : "text-slate-700";
                            return (
                          <div key={`${event.occurred_at}-${event.event_type}-${index}`} className="rounded-lg border border-slate-100 bg-slate-50 px-3 py-2">
                            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                              <span className={`font-semibold ${titleCls}`}>{formatLeaderEventLabel(event.event_type)}</span>
                              <span className="text-slate-500">{formatDiagnosticTime(event.occurred_at)}</span>
                              <span className="text-slate-500">epoch {event.leader_epoch ?? "—"}</span>
                            </div>
                            <p className="mt-1 break-all text-xs text-slate-500">{event.owner_id}</p>
                            {event.detail ? <p className="mt-1 text-xs text-slate-500">{event.detail}</p> : null}
                          </div>
                            );
                          })}
                      </div>
                    ) : (
                      <p className="text-sm text-slate-500">当前还没有记录到 leader 切换事件。</p>
                    )}
                  </div>
                </div>
              )}
            </SGroup>
          </div>
        )
      )}
    </div>
  );

  const guideDialog = (
    <KnowledgeGuideDialog open={knowledgeGuideOpen} status={knowledgeStatus} onClose={() => setKnowledgeGuideOpen(false)} />
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
      <div className="fixed inset-0 bg-black/60 z-50 flex justify-end" onClick={() => { void handleCloseSettings(); }}>
        <div
          className="w-full max-w-sm bg-white border-l border-slate-200 h-full overflow-y-auto shadow-xl"
          onClick={e => e.stopPropagation()}
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
