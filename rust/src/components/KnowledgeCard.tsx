import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { KnowledgeStatus } from "@/services/tauriApi";

export function KnowledgeCard() {
  const [knowledge, setKnowledge] = useState<KnowledgeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

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
        </>
      )}
    </section>
  );
}
