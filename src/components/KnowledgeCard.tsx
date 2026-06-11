import { useEffect, useRef, useState } from "react";
import {
  Badge,
  Button,
  Card,
  CardSection,
  Field,
  Notice,
} from "@/components/ui";
import { api } from "@/services/api";
import { useJobProgress } from "@/hooks/useJobProgress";
import type {
  ExportPackStats,
  ImportPackStats,
  KnowledgeStatus,
  SubmitJobAck,
} from "@/services/tauriApi";

export function KnowledgeCard() {
  const [knowledge, setKnowledge] = useState<KnowledgeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [packPath, setPackPath] = useState("");
  const [packMsg, setPackMsg] = useState<string | null>(null);
  const [packBusy, setPackBusy] = useState(false);
  const [overwriteOnImport, setOverwriteOnImport] = useState(false);

  const [dllPath, setDllPath] = useState("");
  // 默认关闭：baselib-fetch 走 GitHub API，国内网络经常卡住；
  // 用户需要时勾上即可。
  const [includeBaselib, setIncludeBaselib] = useState(false);
  const [force, setForce] = useState(false);
  const [refreshBusy, setRefreshBusy] = useState(false);
  const [refreshStage, setRefreshStage] = useState<string | null>(null);
  const [refreshMsg, setRefreshMsg] = useState<string | null>(null);
  const [refreshJobId, setRefreshJobId] = useState<string | null>(null);
  const refreshJobIdRef = useRef<string | null>(null);

  useEffect(() => {
    refreshJobIdRef.current = refreshJobId;
  }, [refreshJobId]);

  useEffect(() => {
    (api.getKnowledgeStatus() as Promise<KnowledgeStatus>)
      .then(setKnowledge)
      .catch((e: unknown) => setError(String(e)));
  }, []);

useJobProgress(refreshJobIdRef, (ev) => {
    setRefreshStage(ev.stage);
    if (ev.message) setRefreshMsg(ev.message);
    if (ev.stage === "completed" || ev.stage === "failed" || ev.stage.includes("error")) {
      setRefreshBusy(false);
      void (async () => {
        try {
          const next = (await api.getKnowledgeStatus()) as KnowledgeStatus;
          setKnowledge(next);
        } catch { /* 状态刷新失败不致命 */ }
      })();
    }
  });

  async function handleRefresh() {
    if (!dllPath.trim()) {
      setError("先填 sts2.dll 路径");
      return;
    }
    setError(null);
    setRefreshMsg(null);
    setRefreshStage("submitting");
    setRefreshBusy(true);
    try {
      const ack = (await api.submitKnowledgeRefreshJob({
        sts2_dll_path: dllPath.trim(),
        force,
        include_baselib: includeBaselib,
      })) as SubmitJobAck;
      setRefreshJobId(ack.jobId);
    } catch (e: unknown) {
      setError(String(e));
      setRefreshBusy(false);
      setRefreshStage(null);
    }
  }

  async function handleRecheck() {
    setChecking(true);
    setError(null);
    try {
      const next = (await api.checkKnowledgeStatus()) as KnowledgeStatus;
      setKnowledge(next);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setChecking(false);
    }
  }

  async function handleExport() {
    if (!packPath.trim()) {
      setPackMsg("先填写一个目标 .zip 路径");
      return;
    }
    setPackBusy(true);
    setPackMsg(null);
    try {
      const stats = (await api.exportKnowledgePack(
        packPath.trim(),
      )) as ExportPackStats;
      setPackMsg(
        `✓ 导出成功：${stats.gameFiles} game 文件 + ${stats.baselibIncluded ? "baselib" : "无 baselib"}，${stats.zipBytes} bytes → ${stats.outputPath}`,
      );
    } catch (e: unknown) {
      setPackMsg(`✗ 导出失败：${String(e)}`);
    } finally {
      setPackBusy(false);
    }
  }

  async function handleImport() {
    if (!packPath.trim()) {
      setPackMsg("先填写一个源 .zip 路径");
      return;
    }
    setPackBusy(true);
    setPackMsg(null);
    try {
      const stats = (await api.importKnowledgePack(
        packPath.trim(),
        overwriteOnImport,
      )) as ImportPackStats;
      setPackMsg(
        `✓ 导入成功：${stats.gameFilesWritten} game 文件 + baselib=${stats.baselibWritten} + manifest=${stats.manifestReplaced}`,
      );
      const next = (await api.getKnowledgeStatus()) as KnowledgeStatus;
      setKnowledge(next);
    } catch (e: unknown) {
      setPackMsg(`✗ 导入失败：${String(e)}`);
    } finally {
      setPackBusy(false);
    }
  }

  const overallVariant =
    knowledge?.overall === "fresh"
      ? "ok"
      : knowledge?.overall === "stale"
        ? "warn"
        : "error";

  return (
    <Card
      eyebrow="knowledge · sts2 sources"
      title="Knowledge"
      actions={
        <Button size="sm" onClick={handleRecheck} disabled={checking}>
          {checking ? "Checking…" : "Re-check"}
        </Button>
      }
    >
      {error && <Notice variant="error" title={`Error: ${error}`} />}
      {!error && !knowledge && (
        <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Loading…</p>
      )}

      {knowledge && (
        <>
          <div className="flex items-center gap-3 flex-wrap mb-4">
            <Badge variant={overallVariant}>{knowledge.overall}</Badge>
            <span style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
              root <code>{knowledge.knowledgeRoot}</code>
            </span>
          </div>

          <div className="grid grid-cols-2 gap-3 mb-3">
            {[
              { label: "Game", obj: knowledge.game },
              { label: "BaseLib", obj: knowledge.baselib },
            ].map(({ label, obj }) => (
              <div
                key={label}
                className="p-3"
                style={{
                  background: "var(--paper)",
                  border: "1px solid var(--rule-soft)",
                  borderRadius: "4px",
                }}
              >
                <p
                  style={{
                    fontFamily: '"JetBrains Mono", monospace',
                    fontSize: "10px",
                    letterSpacing: "0.16em",
                    textTransform: "uppercase",
                    color: "var(--ink-mute)",
                    marginBottom: "6px",
                  }}
                >
                  {label}
                </p>
                <p style={{ fontSize: "12px" }}>
                  <span style={{ color: "var(--ink-mute)" }}>mode </span>
                  <code>{obj.sourceMode}</code>
                </p>
                <p style={{ fontSize: "12px" }}>
                  <span style={{ color: "var(--ink-mute)" }}>has .cs </span>
                  <code>{String(obj.hasDecompiledSources)}</code>
                </p>
              </div>
            ))}
          </div>

          {knowledge.warnings.length > 0 && (
            <Notice variant="warn" title="Warnings" className="mb-3">
              <ul className="space-y-0.5">
                {knowledge.warnings.map((w, i) => (
                  <li key={i} style={{ fontSize: "12px" }}>• {w}</li>
                ))}
              </ul>
            </Notice>
          )}

          <p
            style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}
            className="mb-1"
          >
            Embedded templates ({knowledge.embeddedTemplates.length}):
          </p>
          <p
            style={{
              fontFamily: '"JetBrains Mono", monospace',
              fontSize: "11px",
              color: "var(--ink-soft)",
            }}
          >
            {knowledge.embeddedTemplates.join(", ")}
          </p>

          {__IS_TAURI__ && (
            <CardSection title="Refresh (ilspycmd)">
              <p
                style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}
                className="mb-2"
              >
                需要本机装了 <code>ilspycmd</code>（<code>dotnet tool install -g ilspycmd</code>）。
              </p>
              <div className="space-y-2">
                <Field label="sts2.dll 路径">
                  <input
                    value={dllPath}
                    onChange={(e) => setDllPath(e.target.value)}
                    placeholder="C:/Program Files (x86)/Steam/steamapps/common/Slay the Spire 2/data_sts2_windows_x86_64/sts2.dll"
                    className="input-mono"
                  />
                </Field>
                <div
                  className="flex flex-wrap items-center gap-4"
                  style={{ fontSize: "13px" }}
                >
                  <label
                    className="flex items-center gap-2"
                    title="勾上会从 GitHub 拉 BaseLib.dll —— 国内网络可能卡住"
                  >
                    <input
                      type="checkbox"
                      checked={includeBaselib}
                      onChange={(e) => setIncludeBaselib(e.target.checked)}
                    />
                    <span>include_baselib（需访问 GitHub）</span>
                  </label>
                  <label className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      checked={force}
                      onChange={(e) => setForce(e.target.checked)}
                    />
                    <span>force（跳过 manifest 缓存）</span>
                  </label>
                </div>
                <Button
                  variant="accent"
                  size="sm"
                  onClick={handleRefresh}
                  disabled={refreshBusy}
                >
                  {refreshBusy ? "Refreshing…" : "Refresh game library"}
                </Button>
                {(refreshStage || refreshMsg) && (
                  <p style={{ fontSize: "11.5px" }}>
                    <span style={{ color: "var(--ink-mute)" }}>stage </span>
                    <code>{refreshStage}</code>
                    {refreshMsg && (
                      <span
                        className="break-all ml-2"
                        style={{ color: "var(--ink-mute)" }}
                      >
                        — {refreshMsg}
                      </span>
                    )}
                  </p>
                )}
              </div>
            </CardSection>
          )}

          {__IS_TAURI__ && (
            <CardSection title="Knowledge pack">
              <div className="space-y-2">
                <Field label="ZIP 路径（导出目标 / 导入源）">
                  <input
                    value={packPath}
                    onChange={(e) => setPackPath(e.target.value)}
                    placeholder="E:/share/sts2-knowledge.zip"
                    className="input-mono"
                  />
                </Field>
                <label
                  className="flex items-center gap-2"
                  style={{ fontSize: "13px" }}
                >
                  <input
                    type="checkbox"
                    checked={overwriteOnImport}
                    onChange={(e) => setOverwriteOnImport(e.target.checked)}
                  />
                  <span>导入时覆盖现有 game/baselib（默认拒绝避免误操作）</span>
                </label>
                <div className="flex gap-2">
                  <Button
                    variant="accent"
                    size="sm"
                    onClick={handleExport}
                    disabled={packBusy}
                  >
                    Export
                  </Button>
                  <Button
                    variant="accent"
                    size="sm"
                    onClick={handleImport}
                    disabled={packBusy}
                  >
                    Import
                  </Button>
                </div>
                {packMsg && (
                  <p
                    className="whitespace-pre-wrap break-all"
                    style={{ fontSize: "11.5px" }}
                  >
                    {packMsg}
                  </p>
                )}
              </div>
            </CardSection>
          )}
        </>
      )}
    </Card>
  );
}
