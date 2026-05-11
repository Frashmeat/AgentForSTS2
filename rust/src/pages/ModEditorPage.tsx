// Mod Editor 页：当前 active project 的快速概览 —— mod_analyzer 输出 +
// 关键路径 + 一键打开 artifacts/items/history。
//
// 简化版：不做完整文件树 + .cs 高亮（CodeMirror 集成是后续工作），先满足
// "用户看一眼这个 mod 项目长啥样、缺什么、跑过几个 item"。

import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type {
  ArtifactStatus,
  ModAnalysisReport,
  ProjectSnapshot,
} from "@/services/tauriApi";

export function ModEditorPage() {
  const [project, setProject] = useState<ProjectSnapshot | null>(null);
  const [report, setReport] = useState<ModAnalysisReport | null>(null);
  const [artifacts, setArtifacts] = useState<ArtifactStatus[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [analyzing, setAnalyzing] = useState(false);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void (async () => {
      try {
        const snap = (await api.currentProject()) as ProjectSnapshot | null;
        setProject(snap);
        if (snap) {
          void refreshAll(snap.path);
        }
      } catch (e: unknown) {
        setError(String(e));
      }
    })();
  }, []);

  async function refreshAll(projectRoot: string) {
    setAnalyzing(true);
    setError(null);
    try {
      const [r, a] = await Promise.all([
        api.analyzeModProject(projectRoot) as Promise<ModAnalysisReport>,
        api.planArtifactList() as Promise<ArtifactStatus[]>,
      ]);
      setReport(r);
      setArtifacts(a);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setAnalyzing(false);
    }
  }

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h1 className="text-2xl font-semibold mb-2">Mod Editor</h1>
        <p className="text-muted text-sm">桌面端 only —— 需访问本地工程文件。</p>
      </section>
    );
  }

  if (!project) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h1 className="text-2xl font-semibold mb-2">Mod Editor</h1>
        <p className="text-amber-600 text-sm">
          没有打开的工程。先去 Dashboard → Project 卡片新建或打开一个。
        </p>
      </section>
    );
  }

  return (
    <div className="space-y-4">
      <header className="flex items-start justify-between">
        <div>
          <h1 className="text-2xl font-semibold">Mod Editor</h1>
          <p className="text-muted text-sm">
            <code>{project.path}</code>
          </p>
        </div>
        <button
          type="button"
          onClick={() => refreshAll(project.path)}
          disabled={analyzing}
          className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
        >
          {analyzing ? "Analyzing…" : "Re-analyze"}
        </button>
      </header>

      {error && <p className="text-red-500 text-sm">Error: {error}</p>}

      {report && (
        <section className="rounded border border-muted/30 p-4 space-y-2">
          <h2 className="text-lg font-medium">Project structure</h2>
          <dl className="text-sm grid grid-cols-[160px_1fr] gap-x-2 gap-y-1">
            <dt className="text-muted">.csproj</dt>
            <dd>
              <code className="text-xs break-all">{report.csprojPath ?? "(none)"}</code>
            </dd>
            {report.csprojSummary && (
              <>
                <dt className="text-muted">SDK</dt>
                <dd>{report.csprojSummary.sdk ?? "(none)"}</dd>
                <dt className="text-muted">TargetFramework</dt>
                <dd>{report.csprojSummary.targetFramework ?? "(none)"}</dd>
                <dt className="text-muted">PackageReferences</dt>
                <dd>
                  {report.csprojSummary.packageReferences.length === 0 ? (
                    <span className="text-muted">(none)</span>
                  ) : (
                    <ul className="space-y-0.5">
                      {report.csprojSummary.packageReferences.map((p) => (
                        <li key={p} className="text-xs">
                          <code>{p}</code>
                        </li>
                      ))}
                    </ul>
                  )}
                </dd>
              </>
            )}
            <dt className="text-muted">.cs files</dt>
            <dd>
              {report.csFilesCount} files · {report.csTotalBytes} bytes
            </dd>
            <dt className="text-muted">artifacts/ count</dt>
            <dd>{report.artifactsCount}</dd>
          </dl>
          {report.modMeta && (
            <details className="text-sm">
              <summary className="cursor-pointer text-muted">packages.json</summary>
              <dl className="grid grid-cols-[100px_1fr] gap-x-2 gap-y-1 mt-1 text-xs">
                <dt className="text-muted">name</dt>
                <dd>{report.modMeta.name ?? "-"}</dd>
                <dt className="text-muted">author</dt>
                <dd>{report.modMeta.author ?? "-"}</dd>
                <dt className="text-muted">version</dt>
                <dd>{report.modMeta.version ?? "-"}</dd>
              </dl>
            </details>
          )}
          {report.warnings.length > 0 && (
            <ul className="text-xs text-amber-700 space-y-0.5 mt-2">
              {report.warnings.map((w, i) => (
                <li key={i}>⚠ {w}</li>
              ))}
            </ul>
          )}
        </section>
      )}

      <section className="rounded border border-muted/30 p-4 space-y-2">
        <h2 className="text-lg font-medium">PlanItem artifacts</h2>
        {artifacts.length === 0 ? (
          <p className="text-muted text-sm">
            没有 plan_artifact 状态记录。通过 audit 接口或后续 batch UI 写入。
          </p>
        ) : (
          <table className="text-sm w-full">
            <thead className="text-xs text-muted">
              <tr>
                <th className="text-left pb-1">item_id</th>
                <th className="text-left pb-1">state</th>
                <th className="text-left pb-1">updated</th>
                <th className="text-left pb-1">cs / png</th>
              </tr>
            </thead>
            <tbody>
              {artifacts.map((a) => (
                <tr key={a.itemId} className="border-t border-muted/10">
                  <td className="py-1">
                    <code className="text-xs">{a.itemId}</code>
                  </td>
                  <td>
                    <span
                      className={
                        a.state === "reviewed" || a.state === "generated"
                          ? "text-emerald-600"
                          : a.state === "failed"
                            ? "text-red-600"
                            : "text-muted"
                      }
                    >
                      {a.state}
                    </span>
                  </td>
                  <td className="text-xs text-muted">
                    {new Date(a.updatedAt).toLocaleString()}
                  </td>
                  <td className="text-xs text-muted">
                    {a.csPath ? "✓ cs" : ""} {a.pngPath ? "✓ png" : ""}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
