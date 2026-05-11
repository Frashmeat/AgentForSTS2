import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type {
  ExportPackStats,
  ImportPackStats,
  KnowledgeStatus,
} from "@/services/tauriApi";

export function KnowledgeCard() {
  const [knowledge, setKnowledge] = useState<KnowledgeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [packPath, setPackPath] = useState("");
  const [packMsg, setPackMsg] = useState<string | null>(null);
  const [packBusy, setPackBusy] = useState(false);
  const [overwriteOnImport, setOverwriteOnImport] = useState(false);

  useEffect(() => {
    (api.getKnowledgeStatus() as Promise<KnowledgeStatus>)
      .then(setKnowledge)
      .catch((e: unknown) => setError(String(e)));
  }, []);

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
      const stats = (await api.exportKnowledgePack(packPath.trim())) as ExportPackStats;
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
      // 刷新状态显示
      const next = (await api.getKnowledgeStatus()) as KnowledgeStatus;
      setKnowledge(next);
    } catch (e: unknown) {
      setPackMsg(`✗ 导入失败：${String(e)}`);
    } finally {
      setPackBusy(false);
    }
  }

  const overallColor =
    knowledge?.overall === "fresh"
      ? "text-emerald-600"
      : knowledge?.overall === "stale"
        ? "text-amber-600"
        : "text-red-600";

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-2">
        <h2 className="text-lg font-medium">Knowledge</h2>
        <button
          type="button"
          onClick={handleRecheck}
          disabled={checking}
          className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
        >
          {checking ? "Checking…" : "Re-check"}
        </button>
      </div>
      {error && <p className="text-red-500">Error: {error}</p>}
      {!error && !knowledge && <p className="text-muted">Loading…</p>}
      {knowledge && (
        <>
          <p className="mb-3">
            <span className="text-muted text-sm">Overall: </span>
            <span className={`font-medium ${overallColor}`}>{knowledge.overall}</span>
            <span className="text-muted text-sm ml-4">Root: </span>
            <code className="text-xs">{knowledge.knowledgeRoot}</code>
          </p>

          <div className="grid grid-cols-2 gap-3 text-sm mb-3">
            <div className="border border-muted/20 rounded p-2">
              <p className="font-medium mb-1">Game</p>
              <p className="text-muted text-xs">
                Mode: <span className="font-mono">{knowledge.game.sourceMode}</span>
              </p>
              <p className="text-muted text-xs">
                Has .cs sources:{" "}
                <span className="font-mono">
                  {String(knowledge.game.hasDecompiledSources)}
                </span>
              </p>
            </div>
            <div className="border border-muted/20 rounded p-2">
              <p className="font-medium mb-1">BaseLib</p>
              <p className="text-muted text-xs">
                Mode: <span className="font-mono">{knowledge.baselib.sourceMode}</span>
              </p>
              <p className="text-muted text-xs">
                Has decompiled.cs:{" "}
                <span className="font-mono">
                  {String(knowledge.baselib.hasDecompiledSources)}
                </span>
              </p>
            </div>
          </div>

          {knowledge.warnings.length > 0 && (
            <div className="rounded border border-amber-500/40 bg-amber-50/40 p-2 mb-3">
              <p className="text-amber-700 text-sm font-medium mb-1">Warnings</p>
              <ul className="text-xs space-y-0.5 text-amber-800">
                {knowledge.warnings.map((w, i) => (
                  <li key={i}>• {w}</li>
                ))}
              </ul>
            </div>
          )}

          <p className="text-xs text-muted">
            Embedded templates ({knowledge.embeddedTemplates.length}):{" "}
            <span className="font-mono">
              {knowledge.embeddedTemplates.join(", ")}
            </span>
          </p>

          {__IS_TAURI__ && (
            <div className="mt-4 pt-3 border-t border-muted/20 space-y-2">
              <p className="text-sm font-medium">Knowledge pack</p>
              <label className="flex flex-col gap-1 text-sm">
                <span className="text-muted text-xs">ZIP 路径（导出目标 / 导入源）</span>
                <input
                  value={packPath}
                  onChange={(e) => setPackPath(e.target.value)}
                  placeholder="E:/share/sts2-knowledge.zip"
                  className="px-2 py-1 rounded border border-muted/30 bg-transparent font-mono text-xs"
                />
              </label>
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={overwriteOnImport}
                  onChange={(e) => setOverwriteOnImport(e.target.checked)}
                />
                <span>导入时覆盖现有 game/baselib（默认拒绝避免误操作）</span>
              </label>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={handleExport}
                  disabled={packBusy}
                  className="text-xs px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
                >
                  Export
                </button>
                <button
                  type="button"
                  onClick={handleImport}
                  disabled={packBusy}
                  className="text-xs px-3 py-1 rounded border border-accent/60 text-accent hover:bg-accent/10 disabled:opacity-50"
                >
                  Import
                </button>
              </div>
              {packMsg && (
                <p className="text-xs whitespace-pre-wrap break-all">{packMsg}</p>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
}
