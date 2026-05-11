import { useEffect, useRef, useState } from "react";
import { api } from "@/services/api";
import type {
  Job,
  JobProgressEvent,
  JobStatus,
  JobSummary,
  SubmitJobAck,
} from "@/services/tauriApi";

const STATUS_COLOR: Record<JobStatus, string> = {
  pending: "text-muted",
  running: "text-accent",
  completed: "text-emerald-600",
  failed: "text-red-600",
  cancelled: "text-amber-600",
};

type SubmitKind =
  | "text_generate"
  | "code_generate_asset"
  | "code_generate_custom"
  | "asset_generate"
  | "batch_custom_code"
  | "build_project"
  | "package_project"
  | "log_analysis"
  | "knowledge_refresh";

const KIND_LABELS: Array<{ value: SubmitKind; label: string }> = [
  { value: "text_generate", label: "text_generate (free prompt)" },
  { value: "code_generate_asset", label: "code_generate (asset)" },
  { value: "code_generate_custom", label: "code_generate (custom code)" },
  { value: "asset_generate", label: "asset_generate (image+code)" },
  { value: "batch_custom_code", label: "batch_custom_code" },
  { value: "build_project", label: "build_project (dotnet publish)" },
  { value: "package_project", label: "package_project (zip)" },
  { value: "log_analysis", label: "log_analysis (LLM diagnose)" },
  { value: "knowledge_refresh", label: "knowledge_refresh (ilspycmd)" },
];

export function JobsCard() {
  const [list, setList] = useState<JobSummary[]>([]);
  const [active, setActive] = useState<Job | null>(null);
  const [submitKind, setSubmitKind] = useState<SubmitKind>("text_generate");

  // 各 kind 的字段，分散放置（明确好读）
  const [prompt, setPrompt] = useState("用一句中文打招呼");
  const [assetType, setAssetType] = useState("card");
  const [assetName, setAssetName] = useState("DemoCard");
  const [designDescription, setDesignDescription] = useState(
    "造成 10 点伤害，弃 1 张牌。",
  );
  const [assetProjectRoot, setAssetProjectRoot] = useState("");
  const [customName, setCustomName] = useState("MyHook");
  const [customDescription, setCustomDescription] = useState("把 player.maxHp 翻倍");
  const [customImplNotes, setCustomImplNotes] = useState(
    "OverrideMember Player.GetMaxHp 返回原值 *2",
  );
  const [imagePrompt, setImagePrompt] = useState(
    "a fierce-looking card art, dark background, fantasy style",
  );
  const [buildProjectRoot, setBuildProjectRoot] = useState("");
  const [packageSourceDir, setPackageSourceDir] = useState("");
  const [packageOutputPath, setPackageOutputPath] = useState("");
  const [logText, setLogText] = useState("");
  const [logContextHint, setLogContextHint] = useState("");
  const [batchItemsJson, setBatchItemsJson] = useState(
    `[
  {
    "name": "HookOne",
    "description": "把 player 起手获得 1 点护甲",
    "implementation_notes": "OnBattleStart hook 加 BlockPower 1",
    "project_root": "",
    "skip_build": true
  }
]`,
  );
  const [batchFailFast, setBatchFailFast] = useState(false);
  const [knowledgeDllPath, setKnowledgeDllPath] = useState("");
  const [knowledgeForce, setKnowledgeForce] = useState(false);
  const [knowledgeIncludeBaselib, setKnowledgeIncludeBaselib] = useState(false);

  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [liveDeltaById, setLiveDeltaById] = useState<Record<string, string>>({});
  const unlistenRef = useRef<(() => void) | null>(null);

  async function refresh() {
    try {
      const items = (await api.listJobs()) as JobSummary[];
      setList(items);
    } catch (e: unknown) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    if (!__IS_TAURI__) return;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<JobProgressEvent>("job-progress", (e) => {
        const ev = e.payload;
        if (ev.delta) {
          setLiveDeltaById((prev) => ({
            ...prev,
            [ev.jobId]: (prev[ev.jobId] ?? "") + ev.delta,
          }));
        }
        if (
          ev.stage === "completed" ||
          ev.stage.includes("error") ||
          ev.stage.includes("cancel") ||
          ev.stage === "failed"
        ) {
          void refresh();
          setActive((cur) => {
            if (cur && cur.id === ev.jobId) {
              void (async () => {
                try {
                  const next = (await api.getJob(ev.jobId)) as Job;
                  setActive(next);
                } catch {
                  // ignore
                }
              })();
            }
            return cur;
          });
        }
      });
      unlistenRef.current = stop;
    })();
    return () => {
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, []);

  async function handleSubmit() {
    setBusy(true);
    setError(null);
    try {
      let ack: SubmitJobAck;
      switch (submitKind) {
        case "text_generate":
          ack = (await api.submitTextGenerateJob({ prompt })) as SubmitJobAck;
          break;
        case "code_generate_asset":
          ack = (await api.submitCodeGenerateJob({
            mode: "asset",
            request: {
              asset_type: assetType,
              asset_name: assetName,
              design_description: designDescription,
              project_root: assetProjectRoot || ".",
              image_paths: [],
              name_zhs: "",
              skip_build: true,
            },
          })) as SubmitJobAck;
          break;
        case "code_generate_custom":
          ack = (await api.submitCodeGenerateJob({
            mode: "custom_code",
            request: {
              name: customName,
              description: customDescription,
              implementation_notes: customImplNotes,
              project_root: assetProjectRoot || ".",
              skip_build: true,
            },
          })) as SubmitJobAck;
          break;
        case "asset_generate":
          ack = (await api.submitAssetGenerateJob({
            asset_request: {
              asset_type: assetType,
              asset_name: assetName,
              design_description: designDescription,
              project_root: assetProjectRoot || ".",
              image_paths: [],
              name_zhs: "",
              skip_build: true,
            },
            image_prompt: imagePrompt.trim() || null,
          })) as SubmitJobAck;
          break;
        case "batch_custom_code": {
          let items;
          try {
            items = JSON.parse(batchItemsJson);
          } catch (e) {
            throw new Error(`batch items JSON parse failed: ${String(e)}`);
          }
          if (!Array.isArray(items)) {
            throw new Error("batch items must be a JSON array");
          }
          ack = (await api.submitBatchCustomCodeJob({
            items,
            fail_fast: batchFailFast,
          })) as SubmitJobAck;
          break;
        }
        case "build_project":
          ack = (await api.submitBuildProjectJob({
            project_root: buildProjectRoot,
            max_attempts: 3,
          })) as SubmitJobAck;
          break;
        case "package_project":
          ack = (await api.submitPackageProjectJob({
            source_dir: packageSourceDir,
            output_path: packageOutputPath.trim() || null,
          })) as SubmitJobAck;
          break;
        case "log_analysis":
          ack = (await api.submitLogAnalysisJob({
            log_text: logText.trim() || null,
            context_hint: logContextHint.trim() || null,
          })) as SubmitJobAck;
          break;
        case "knowledge_refresh":
          ack = (await api.submitKnowledgeRefreshJob({
            sts2_dll_path: knowledgeDllPath,
            force: knowledgeForce,
            include_baselib: knowledgeIncludeBaselib,
          })) as SubmitJobAck;
          break;
      }
      setLiveDeltaById((prev) => ({ ...prev, [ack.jobId]: "" }));
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleSelect(id: string) {
    try {
      const job = (await api.getJob(id)) as Job;
      setActive(job);
    } catch (e: unknown) {
      setError(String(e));
    }
  }

  async function handleCancel(id: string) {
    try {
      await api.cancelJob(id);
      await refresh();
    } catch (e: unknown) {
      setError(String(e));
    }
  }

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Jobs</h2>
        <p className="text-muted text-sm">
          Platform jobs are desktop-only for now. Web sqlx repository lands in stage 3.1a.
        </p>
      </section>
    );
  }

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-medium">Jobs — submit any handler</h2>
        <button
          type="button"
          onClick={refresh}
          disabled={busy}
          className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
        >
          Refresh
        </button>
      </div>

      {error && <p className="text-red-500 text-sm mb-2">Error: {error}</p>}

      <div className="mb-4 space-y-2">
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-muted text-xs">Job kind</span>
          <select
            value={submitKind}
            onChange={(e) => setSubmitKind(e.target.value as SubmitKind)}
            className="px-2 py-1 rounded border border-muted/30 bg-transparent"
          >
            {KIND_LABELS.map((k) => (
              <option key={k.value} value={k.value}>
                {k.label}
              </option>
            ))}
          </select>
        </label>

        {submitKind === "text_generate" && (
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Prompt</span>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={2}
              className="px-2 py-1 rounded border border-muted/30 bg-transparent"
            />
          </label>
        )}

        {(submitKind === "code_generate_asset" || submitKind === "asset_generate") && (
          <>
            <div className="grid grid-cols-2 gap-2 text-sm">
              <label className="flex flex-col gap-1">
                <span className="text-muted text-xs">Asset type</span>
                <select
                  value={assetType}
                  onChange={(e) => setAssetType(e.target.value)}
                  className="px-2 py-1 rounded border border-muted/30 bg-transparent"
                >
                  {["card", "card_fullscreen", "relic", "power", "character"].map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-muted text-xs">Asset name</span>
                <input
                  value={assetName}
                  onChange={(e) => setAssetName(e.target.value)}
                  className="px-2 py-1 rounded border border-muted/30 bg-transparent"
                />
              </label>
            </div>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Project root (for prompt context)</span>
              <input
                value={assetProjectRoot}
                onChange={(e) => setAssetProjectRoot(e.target.value)}
                placeholder="E:/mods/demo_mod"
                className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Design description</span>
              <textarea
                value={designDescription}
                onChange={(e) => setDesignDescription(e.target.value)}
                rows={2}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              />
            </label>
            {submitKind === "asset_generate" && (
              <label className="flex flex-col gap-1 text-sm">
                <span className="text-muted text-xs">
                  Image prompt（留空跳过出图，仅跑 code）
                </span>
                <textarea
                  value={imagePrompt}
                  onChange={(e) => setImagePrompt(e.target.value)}
                  rows={2}
                  className="px-2 py-1 rounded border border-muted/30 bg-transparent"
                />
              </label>
            )}
            <p className="text-xs text-muted">
              Output → <code>artifacts/{assetName}/{assetName}.cs</code>
              {submitKind === "asset_generate" && imagePrompt.trim() && (
                <>
                  {" "}
                  + <code>{assetName}.png</code>
                </>
              )}
            </p>
          </>
        )}

        {submitKind === "code_generate_custom" && (
          <>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Name (sanitized as class name)</span>
              <input
                value={customName}
                onChange={(e) => setCustomName(e.target.value)}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Project root</span>
              <input
                value={assetProjectRoot}
                onChange={(e) => setAssetProjectRoot(e.target.value)}
                placeholder="E:/mods/demo_mod"
                className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Description</span>
              <textarea
                value={customDescription}
                onChange={(e) => setCustomDescription(e.target.value)}
                rows={2}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Implementation notes</span>
              <textarea
                value={customImplNotes}
                onChange={(e) => setCustomImplNotes(e.target.value)}
                rows={2}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              />
            </label>
          </>
        )}

        {submitKind === "batch_custom_code" && (
          <>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">
                Items (JSON array of CustomCodegenRequest)
              </span>
              <textarea
                value={batchItemsJson}
                onChange={(e) => setBatchItemsJson(e.target.value)}
                rows={8}
                className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
              />
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={batchFailFast}
                onChange={(e) => setBatchFailFast(e.target.checked)}
              />
              <span>fail_fast（首失立停）</span>
            </label>
          </>
        )}

        {submitKind === "build_project" && (
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-muted text-xs">Project root (will run `dotnet publish` here)</span>
            <input
              value={buildProjectRoot}
              onChange={(e) => setBuildProjectRoot(e.target.value)}
              placeholder="E:/mods/demo_mod/DemoMod"
              className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
            />
          </label>
        )}

        {submitKind === "package_project" && (
          <>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Source dir (要打包的目录)</span>
              <input
                value={packageSourceDir}
                onChange={(e) => setPackageSourceDir(e.target.value)}
                placeholder="E:/mods/demo_mod/artifacts"
                className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">
                Output zip path (留空 → 与 source_dir 同级 &lt;name&gt;-&lt;ts&gt;.zip)
              </span>
              <input
                value={packageOutputPath}
                onChange={(e) => setPackageOutputPath(e.target.value)}
                placeholder="E:/mods/demo_mod-release.zip"
                className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
              />
            </label>
          </>
        )}

        {submitKind === "log_analysis" && (
          <>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Build log（粘贴日志文本）</span>
              <textarea
                value={logText}
                onChange={(e) => setLogText(e.target.value)}
                rows={6}
                placeholder="把 dotnet publish 失败日志粘贴到这里…"
                className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
              />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">Context hint（可选，提示用户改了什么）</span>
              <input
                value={logContextHint}
                onChange={(e) => setLogContextHint(e.target.value)}
                placeholder="我刚改了 TargetFramework 到 net9.0"
                className="px-2 py-1 rounded border border-muted/30 bg-transparent"
              />
            </label>
          </>
        )}

        {submitKind === "knowledge_refresh" && (
          <>
            <label className="flex flex-col gap-1 text-sm">
              <span className="text-muted text-xs">sts2.dll path</span>
              <input
                value={knowledgeDllPath}
                onChange={(e) => setKnowledgeDllPath(e.target.value)}
                placeholder="E:/Steam/steamapps/common/SlayTheSpire2/sts2.dll"
                className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
              />
            </label>
            <div className="flex flex-wrap gap-4 text-sm">
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={knowledgeForce}
                  onChange={(e) => setKnowledgeForce(e.target.checked)}
                />
                <span>force（忽略 manifest 缓存重新反编译）</span>
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={knowledgeIncludeBaselib}
                  onChange={(e) => setKnowledgeIncludeBaselib(e.target.checked)}
                />
                <span>include_baselib（同时拉 BaseLib.dll 反编译）</span>
              </label>
            </div>
          </>
        )}

        <button
          type="button"
          onClick={handleSubmit}
          disabled={busy}
          className="text-sm px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
        >
          Submit
        </button>
      </div>

      {list.length === 0 ? (
        <p className="text-muted text-sm">No jobs yet. Submit one above.</p>
      ) : (
        <ul className="space-y-2 text-sm mb-4">
          {list.map((j) => (
            <li
              key={j.id}
              className="border border-muted/20 rounded p-2 flex items-center justify-between gap-2"
            >
              <div className="min-w-0 flex-1">
                <p>
                  <code className="text-xs">{j.id}</code>{" "}
                  <span className={`ml-2 text-xs font-medium ${STATUS_COLOR[j.status]}`}>
                    {j.status}
                  </span>
                  <span className="ml-2 text-xs text-muted">{j.kind}</span>
                </p>
                <p className="text-xs text-muted">
                  Created {new Date(j.createdAt).toLocaleTimeString()}
                  {j.completedAt && (
                    <>
                      {" · "}Completed {new Date(j.completedAt).toLocaleTimeString()}
                    </>
                  )}
                </p>
                {liveDeltaById[j.id] && j.status === "running" && (
                  <p className="text-xs text-accent truncate">
                    {liveDeltaById[j.id]}
                  </p>
                )}
              </div>
              <button
                type="button"
                onClick={() => handleSelect(j.id)}
                className="text-xs px-2 py-1 rounded border border-muted/40 hover:bg-muted/10"
              >
                View
              </button>
              {j.status === "running" && (
                <button
                  type="button"
                  onClick={() => handleCancel(j.id)}
                  className="text-xs px-2 py-1 rounded border border-amber-500/40 text-amber-600 hover:bg-amber-50/40"
                >
                  Cancel
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {active && (
        <details open className="mt-3">
          <summary className="cursor-pointer text-sm font-medium mb-2">
            Job {active.id} ({active.status})
          </summary>
          <pre className="text-xs p-3 rounded border border-muted/20 overflow-auto max-h-96 whitespace-pre-wrap">
            {JSON.stringify(active.result ?? { error: active.error }, null, 2)}
          </pre>
        </details>
      )}
    </section>
  );
}
