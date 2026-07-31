// Mod Editor 页：当前 active project 的快速概览 —— mod_analyzer 输出 +
// 关键路径 + 一键打开 artifacts/items/history。
//
// 简化版：不做完整文件树 + .cs 高亮（CodeMirror 集成是后续工作），先满足
// "用户看一眼这个 mod 项目长啥样、缺什么、跑过几个 item"。

import { useEffect, useState } from "react";
import { useProjectStore } from "@/stores/project";
import {
  Badge,
  Button,
  Card,
  KV,
  KVList,
  Notice,
  PageHero,
} from "@/components/ui";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { api } from "@/services/api";
import { toActionableFailure } from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
import type {
  ArtifactStatus,
  ModAnalysisReport,
} from "@/services/tauriApi";

function stateVariant(s: string): "ok" | "error" | "warn" | "muted" {
  if (s === "reviewed" || s === "generated") return "ok";
  if (s === "failed") return "error";
  if (s === "stale" || s === "needs_input") return "warn";
  return "muted";
}

export function ModEditorPage() {
  const project = useProjectStore((s) => s.project);
  const [report, setReport] = useState<ModAnalysisReport | null>(null);
  const [artifacts, setArtifacts] = useState<ArtifactStatus[]>([]);
  const [error, setError] = useState<ActionableFailure | null>(null);
  const [analyzing, setAnalyzing] = useState(false);

  useEffect(() => {
    if (project) void refreshAll(project.path);
  }, [project]);
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
      setError(toActionableFailure(e));
    } finally {
      setAnalyzing(false);
    }
  }

  if (!__IS_TAURI__) {
    return (
      <div>
        <PageHero
          eyebrow="editor · mod project"
          title="Mod Editor"
          subtitle="桌面端 only —— 需访问本地工程文件。"
        />
      </div>
    );
  }

  if (!project) {
    return (
      <div>
        <PageHero eyebrow="editor · mod project" title="Mod Editor" />
        <Notice variant="warn" title="没有打开的工程">
          先去 Dashboard → Project 卡片新建或打开一个工程。
        </Notice>
      </div>
    );
  }

  return (
    <div>
      <PageHero
        eyebrow="editor · mod project"
        title="Mod Editor"
        subtitle={
          <code style={{ fontSize: "12px" }}>{project.path}</code>
        }
        actions={
          <Button onClick={() => refreshAll(project.path)} disabled={analyzing}>
            {analyzing ? "Analyzing…" : "Re-analyze"}
          </Button>
        }
      />

      <div className="space-y-4">
        <ActionableErrorNotice failure={error} />

        {report && (
          <Card eyebrow="structure · csproj" title="Project structure">
            <KVList>
              <KV k=".csproj">
                <code className="break-all">{report.csprojPath ?? "(none)"}</code>
              </KV>
              {report.csprojSummary && (
                <>
                  <KV k="SDK">{report.csprojSummary.sdk ?? "(none)"}</KV>
                  <KV k="TargetFramework">
                    {report.csprojSummary.targetFramework ?? "(none)"}
                  </KV>
                  <KV k="PackageRefs">
                    {report.csprojSummary.packageReferences.length === 0 ? (
                      <span style={{ color: "var(--ink-faint)" }}>(none)</span>
                    ) : (
                      <ul className="space-y-0.5">
                        {report.csprojSummary.packageReferences.map((p) => (
                          <li key={p} style={{ fontSize: "11.5px" }}>
                            <code>{p}</code>
                          </li>
                        ))}
                      </ul>
                    )}
                  </KV>
                </>
              )}
              <KV k=".cs files">
                {report.csFilesCount} files · {report.csTotalBytes} bytes
              </KV>
              <KV k="artifacts/ count">{report.artifactsCount}</KV>
            </KVList>
            {report.modMeta && (
              <details className="mt-3">
                <summary
                  className="cursor-pointer"
                  style={{
                    fontFamily: '"JetBrains Mono", monospace',
                    fontSize: "10.5px",
                    letterSpacing: "0.16em",
                    textTransform: "uppercase",
                    color: "var(--ink-mute)",
                  }}
                >
                  packages.json
                </summary>
                <KVList variant="narrow" className="mt-2">
                  <KV k="name">{report.modMeta.name ?? "-"}</KV>
                  <KV k="author">{report.modMeta.author ?? "-"}</KV>
                  <KV k="version">{report.modMeta.version ?? "-"}</KV>
                </KVList>
              </details>
            )}
            {report.warnings.length > 0 && (
              <Notice variant="warn" title="Warnings" className="mt-3">
                <ul className="space-y-0.5">
                  {report.warnings.map((w, i) => (
                    <li key={i} style={{ fontSize: "12px" }}>⚠ {w}</li>
                  ))}
                </ul>
              </Notice>
            )}
          </Card>
        )}

        <Card eyebrow="planning · artifacts" title="PlanItem artifacts">
          {artifacts.length === 0 ? (
            <p style={{ color: "var(--ink-faint)", fontSize: "12.5px" }}>
              没有 plan_artifact 状态记录。通过生成流程或后续 batch UI 写入。
            </p>
          ) : (
            <table
              className="w-full"
              style={{ fontSize: "12.5px", borderCollapse: "collapse" }}
            >
              <thead>
                <tr style={{ color: "var(--ink-mute)" }}>
                  {["item_id", "state", "updated", "cs / png"].map((h) => (
                    <th
                      key={h}
                      className="text-left pb-2"
                      style={{
                        fontFamily: '"JetBrains Mono", monospace',
                        fontSize: "9.5px",
                        letterSpacing: "0.16em",
                        textTransform: "uppercase",
                        fontWeight: 500,
                        borderBottom: "1px solid var(--rule-soft)",
                      }}
                    >
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {artifacts.map((a) => (
                  <tr
                    key={a.itemId}
                    style={{ borderBottom: "1px solid var(--rule-hair)" }}
                  >
                    <td className="py-1.5">
                      <code style={{ fontSize: "11.5px" }}>{a.itemId}</code>
                    </td>
                    <td>
                      <Badge variant={stateVariant(a.state)}>{a.state}</Badge>
                    </td>
                    <td style={{ color: "var(--ink-mute)", fontSize: "11.5px" }}>
                      {new Date(a.updatedAt).toLocaleString()}
                    </td>
                    <td style={{ color: "var(--ink-mute)", fontSize: "11.5px" }}>
                      {a.csPath ? "✓ cs " : ""}
                      {a.pngPath ? "✓ png" : ""}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </div>
    </div>
  );
}
