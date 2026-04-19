import { useEffect, useState } from "react";
import { X, FolderOpen, Cpu, Cloud, Gamepad2, Image, Search } from "lucide-react";
import { Link } from "react-router-dom";
import {
  checkKnowledgeStatus,
  cancelDetectAppPathsTask,
  getMyServerPreferences,
  getDetectAppPathsTask,
  getRefreshKnowledgeTask,
  listPlatformExecutionProfiles,
  loadAppConfig,
  loadKnowledgeStatus,
  pickAppPath,
  startDetectAppPaths,
  startRefreshKnowledgeTask,
  type KnowledgeStatus,
  type MyServerPreferenceView,
  updateAppConfig,
  updateMyServerPreferences,
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";
import type { PlatformExecutionProfile } from "../shared/api/platform.ts";
import { useSession } from "../shared/session/hooks.ts";
import { KnowledgeGuideDialog } from "./KnowledgeGuideDialog.tsx";
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
    return preference.default_execution_profile_id;
  }
  return profiles.find((profile) => profile.available && profile.recommended)?.id
    ?? profiles.find((profile) => profile.available)?.id
    ?? null;
}

interface SettingsPanelProps {
  mode?: "drawer" | "page";
  onClose?: () => void;
  onKnowledgeStatusChange?: (status: KnowledgeStatus) => void;
}

export function SettingsPanel({ mode = "drawer", onClose, onKnowledgeStatusChange }: SettingsPanelProps) {
  const { isAuthAvailable, isAuthenticated } = useSession();
  const [activeTab, setActiveTab] = useState<"workspace" | "server">("workspace");
  const [cfg, setCfg] = useState<any>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [detecting, setDetecting] = useState(false);
  const [detectionTaskId, setDetectionTaskId] = useState("");
  const [detectionStep, setDetectionStep] = useState("");
  const [pathNotes, setPathNotes] = useState<string[]>([]);
  const [knowledgeStatus, setKnowledgeStatus] = useState<KnowledgeStatus | null>(null);
  const [knowledgeTaskId, setKnowledgeTaskId] = useState("");
  const [knowledgeChecking, setKnowledgeChecking] = useState(false);
  const [knowledgeStep, setKnowledgeStep] = useState("");
  const [knowledgeNotes, setKnowledgeNotes] = useState<string[]>([]);
  const [knowledgeError, setKnowledgeError] = useState("");
  const [knowledgeGuideOpen, setKnowledgeGuideOpen] = useState(false);
  const [llmKey, setLlmKey] = useState("");
  const [imgKey, setImgKey] = useState("");
  const [imgSecret, setImgSecret] = useState("");
  const [serverProfiles, setServerProfiles] = useState<PlatformExecutionProfile[]>([]);
  const [serverPreference, setServerPreference] = useState<MyServerPreferenceView | null>(null);
  const [selectedServerProfileId, setSelectedServerProfileId] = useState<number | null>(null);
  const [serverLoading, setServerLoading] = useState(false);
  const [serverSaving, setServerSaving] = useState(false);
  const [serverError, setServerError] = useState("");
  const [serverNotice, setServerNotice] = useState("");

  useEffect(() => {
    loadAppConfig().then(setCfg);
    loadKnowledgeStatus()
      .then((status) => {
        setKnowledgeStatus(status);
        onKnowledgeStatusChange?.(status);
      })
      .catch((error) => {
        setKnowledgeError(resolveErrorMessage(error));
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
      return;
    }

    let cancelled = false;
    setServerLoading(true);
    setServerError("");
    void Promise.all([listPlatformExecutionProfiles(), getMyServerPreferences()])
      .then(([profileView, preference]) => {
        if (cancelled) {
          return;
        }
        setServerProfiles(profileView.items);
        setServerPreference(preference);
        setSelectedServerProfileId(pickInitialServerProfileId(profileView.items, preference));
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setServerProfiles([]);
        setServerPreference(null);
        setSelectedServerProfileId(null);
        setServerError(resolveErrorMessage(error));
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
    if (!detectionTaskId) {
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function pollTask() {
      try {
        const snapshot = await getDetectAppPathsTask(detectionTaskId);
        if (cancelled) {
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
        if (!cancelled) {
          setKnowledgeStatus(status);
          onKnowledgeStatusChange?.(status);
        }
        setKnowledgeTaskId("");
      } catch (error) {
        if (cancelled) {
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

  function set(path: string[], value: string | number) {
    setCfg((prev: any) => {
      const next = structuredClone(prev);
      let cur = next;
      for (let i = 0; i < path.length - 1; i++) cur = cur[path[i]];
      cur[path[path.length - 1]] = value;
      return next;
    });
  }

  function handleProviderChange(provider: string) {
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
      setDetectionTaskId(task.task_id);
      setDetectionStep(task.current_step || "检测中");
      setPathNotes(task.notes ?? []);
    } catch (error) {
      setDetectionStep("");
      setPathNotes([`检测失败：${resolveErrorMessage(error)}`]);
      setDetecting(false);
    }
  }

  async function cancelDetectPaths() {
    if (!detectionTaskId) return;
    try {
      const snapshot = await cancelDetectAppPathsTask(detectionTaskId);
      setDetectionStep(snapshot.current_step || "检测已取消");
      setPathNotes(snapshot.notes ?? ["检测已取消"]);
    } catch (error) {
      setPathNotes([`取消检测失败：${resolveErrorMessage(error)}`]);
    }
  }

  async function choosePath(field: SettingsPathField) {
    if (!cfg) return;
    const request = createSettingsPickPathRequest(field, String(cfg[field] ?? ""));
    try {
      const result = await pickAppPath(request);
      if (!result.path) {
        return;
      }
      set([field], result.path);
      setPathNotes([`✓ ${request.title}: ${result.path}`]);
    } catch (error) {
      setPathNotes([`${request.title}失败：${resolveErrorMessage(error)}`]);
    }
  }

  async function save() {
    setSaving(true);
    setSaveError("");
    const body = structuredClone(cfg);
    if (llmKey.trim()) body.llm.api_key = llmKey.trim();
    if (imgKey.trim()) body.image_gen.api_key = imgKey.trim();
    if (imgSecret.trim()) body.image_gen.api_secret = imgSecret.trim();
    try {
      await updateAppConfig(body);
      if (onClose) {
        onClose();
      } else {
        setPathNotes(prev => ["✓ 设置已保存", ...prev.filter(note => note !== "✓ 设置已保存")]);
      }
    } catch (error) {
      setSaveError(resolveErrorMessage(error) || "保存设置失败");
    } finally {
      setSaving(false);
    }
  }

  const currentProvider = cfg?.image_gen?.provider ?? "bfl";
  const models = PROVIDER_MODELS[currentProvider] ?? [];

  const missingPaths = cfg && (!cfg.default_project_root || !cfg.sts2_path);
  const knowledgeBusy = knowledgeChecking || Boolean(knowledgeTaskId);
  const knowledgeCheckProgress = knowledgeChecking ? 100 : 0;
  const knowledgeUpdateProgress = getKnowledgeUpdateProgress(knowledgeStep, Boolean(knowledgeTaskId));

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
      setKnowledgeStatus(status);
      onKnowledgeStatusChange?.(status);
      setKnowledgeNotes(status.warnings ?? []);
      setKnowledgeStep("");
    } catch (error) {
      setKnowledgeError(resolveErrorMessage(error));
      setKnowledgeStep("检查失败");
    } finally {
      setKnowledgeChecking(false);
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
      setKnowledgeTaskId(task.task_id);
      setKnowledgeStep(task.current_step || "更新中");
      setKnowledgeNotes(task.notes ?? []);
    } catch (error) {
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
      setServerPreference(updated);
      setSelectedServerProfileId(pickInitialServerProfileId(serverProfiles, updated));
      setServerNotice(profileId === null ? "已清空默认服务器配置" : "默认服务器配置已保存");
    } catch (error) {
      setServerError(resolveErrorMessage(error));
    } finally {
      setServerSaving(false);
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
        {activeTab === "workspace" && missingPaths && (
          <p className="text-xs text-amber-600 mt-0.5">⚠ 请配置项目路径</p>
        )}
      </div>
      {mode === "drawer" && onClose ? (
        <button onClick={onClose} className="text-slate-400 hover:text-slate-600 transition-colors p-1 rounded-lg hover:bg-slate-100">
          <X size={18} />
        </button>
      ) : null}
    </div>
  );

  const selectedServerProfile = serverProfiles.find((profile) => profile.id === selectedServerProfileId) ?? null;
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
                {detectionStep && (
                  <p className="text-xs text-amber-700">当前进度：{detectionStep}</p>
                )}
                {pathNotes.length > 0 && (
                  <div className="space-y-0.5">
                    {pathNotes.map((n, i) => (
                      <p
                        key={i}
                        className={`text-xs ${
                          n.startsWith("✓")
                            ? "text-green-600"
                            : n.includes("失败")
                              ? "text-red-600"
                              : "text-slate-400"
                        }`}
                      >
                        {n}
                      </p>
                    ))}
                  </div>
                )}
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
                  {knowledgeStep ? <p className="text-xs text-amber-700">当前进度：{knowledgeStep}</p> : null}
                  {knowledgeNotes.length > 0 ? (
                    <div className="space-y-0.5">
                      {knowledgeNotes.map((note, index) => (
                        <p key={`${note}-${index}`} className="text-xs text-slate-500">{note}</p>
                      ))}
                    </div>
                  ) : null}
                  {knowledgeError ? <p className="text-xs text-rose-600">{knowledgeError}</p> : null}
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
                  <input value={llmKey} onChange={e => setLlmKey(e.target.value)} placeholder={cfg.llm?.api_key ? "已设置" : "未设置"} className={inputCls} />
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
                      <input value={imgKey} onChange={e => setImgKey(e.target.value)} placeholder={cfg.image_gen?.api_key ? "已设置" : "未设置"} className={inputCls} />
                    </Field>
                    <Field label="Secret Key（SK，留空不修改）">
                      <input type="password" value={imgSecret} onChange={e => setImgSecret(e.target.value)} placeholder={cfg.image_gen?.api_secret ? "已设置" : "未设置"} className={inputCls} />
                    </Field>
                  </>
                ) : (
                  <Field label="API Key（留空不修改）">
                    <input value={imgKey} onChange={e => setImgKey(e.target.value)} placeholder={cfg.image_gen?.api_key ? "已设置" : "未设置"} className={inputCls} />
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
              </SGroup>

              {saveError ? <p className="text-sm text-rose-600">{saveError}</p> : null}
              <button
                onClick={save}
                disabled={saving}
                className="w-full py-2.5 rounded-lg bg-amber-500 text-white font-bold text-sm hover:bg-amber-600 disabled:opacity-40 transition-colors"
              >
                {saving ? "保存中…" : "保存设置"}
              </button>
        </>
        ) : (
          <div className="space-y-6">
            <SGroup icon={<Cloud size={14} />} title="服务器模式">
              {!isAuthAvailable ? (
                <div className="rounded-xl border border-dashed border-slate-200 bg-slate-50 px-4 py-4 text-sm text-slate-500">
                  当前环境未接入独立 Web 平台服务，暂时无法管理服务器模式默认配置。
                </div>
              ) : !isAuthenticated ? (
                <div className="rounded-xl border border-dashed border-slate-200 bg-slate-50 px-4 py-4 text-sm text-slate-500">
                  <p>登录后即可查看平台提供的执行配置，并设置默认服务器模式。</p>
                  <Link
                    to="/auth/login"
                    className="mt-3 inline-flex rounded-lg border border-amber-300 px-3 py-1.5 text-xs font-medium text-amber-700 transition hover:bg-amber-50"
                  >
                    去登录
                  </Link>
                </div>
              ) : serverLoading ? (
                <p className="text-sm text-slate-500">正在读取服务器执行配置…</p>
              ) : (
                <>
                  {serverPreference?.default_execution_profile_id && !serverPreference.available ? (
                    <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">
                      当前默认服务器配置已不可用。你可以改选一个可用配置后重新保存，或直接清空默认值。
                    </div>
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
                      return (
                        <button
                          key={profile.id}
                          type="button"
                          onClick={() => {
                            setSelectedServerProfileId(profile.id);
                            setServerNotice("");
                          }}
                          className={`w-full rounded-xl border px-4 py-3 text-left transition ${
                            selected
                              ? "border-amber-300 bg-amber-50"
                              : "border-slate-200 hover:border-amber-200 hover:bg-amber-50/40"
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

                  {serverError ? <p className="text-sm text-rose-600">{serverError}</p> : null}
                  {serverNotice ? <p className="text-sm text-emerald-700">{serverNotice}</p> : null}

                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => {
                        void handleSaveServerPreference(selectedServerProfile?.id ?? null);
                      }}
                      disabled={serverSaving || selectedServerProfileId === null}
                      className="rounded-lg bg-amber-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-amber-600 disabled:opacity-40"
                    >
                      {serverSaving ? "保存中…" : "保存默认服务器配置"}
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        void handleSaveServerPreference(null);
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
      <div className="fixed inset-0 bg-black/60 z-50 flex justify-end" onClick={onClose}>
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
