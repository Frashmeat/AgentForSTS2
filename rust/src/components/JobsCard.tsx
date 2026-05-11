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

type SubmitKind = "text_generate" | "code_generate_asset" | "build_project";

export function JobsCard() {
  const [list, setList] = useState<JobSummary[]>([]);
  const [active, setActive] = useState<Job | null>(null);
  const [submitKind, setSubmitKind] = useState<SubmitKind>("text_generate");
  // text_generate fields
  const [prompt, setPrompt] = useState("用一句中文打招呼");
  // code_generate (asset) fields
  const [assetType, setAssetType] = useState("card");
  const [assetName, setAssetName] = useState("DemoCard");
  const [designDescription, setDesignDescription] = useState(
    "造成 10 点伤害，弃 1 张牌。",
  );
  const [assetProjectRoot, setAssetProjectRoot] = useState("");
  // build_project fields
  const [buildProjectRoot, setBuildProjectRoot] = useState("");

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
        if (ev.stage === "completed" || ev.stage.includes("error") || ev.stage.includes("cancel")) {
          void refresh();
          // 若当前查看的就是这条任务，刷新它的完整 Job
          setActive((cur) => {
            if (cur && cur.id === ev.jobId) {
              void (async () => {
                try {
                  const next = (await api.getJob(ev.jobId)) as Job;
                  setActive(next);
                } catch {
                  // 任务被删等情况忽略
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
      if (submitKind === "text_generate") {
        ack = (await api.submitTextGenerateJob({ prompt })) as SubmitJobAck;
      } else if (submitKind === "code_generate_asset") {
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
      } else {
        ack = (await api.submitBuildProjectJob({
          project_root: buildProjectRoot,
          max_attempts: 3,
        })) as SubmitJobAck;
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
        <h2 className="text-lg font-medium">Jobs — Text Generate</h2>
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
            <option value="text_generate">text_generate (free prompt)</option>
            <option value="code_generate_asset">code_generate (asset)</option>
            <option value="build_project">build_project (dotnet publish)</option>
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

        {submitKind === "code_generate_asset" && (
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
            <p className="text-xs text-muted">
              Output → <code>artifacts/{assetName}/{assetName}.cs</code> + raw.md in the active project
            </p>
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
