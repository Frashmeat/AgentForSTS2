import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { HealthReport, KnowledgeStatus } from "@/services/tauriApi";

export default function App() {
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);

  const [knowledge, setKnowledge] = useState<KnowledgeStatus | null>(null);
  const [knowledgeError, setKnowledgeError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    (api.getHealth() as Promise<HealthReport>)
      .then(setHealth)
      .catch((e: unknown) => setHealthError(String(e)));
    (api.getKnowledgeStatus() as Promise<KnowledgeStatus>)
      .then(setKnowledge)
      .catch((e: unknown) => setKnowledgeError(String(e)));
  }, []);

  async function handleRecheck() {
    setChecking(true);
    setKnowledgeError(null);
    try {
      const next = (await api.checkKnowledgeStatus()) as KnowledgeStatus;
      setKnowledge(next);
    } catch (e: unknown) {
      setKnowledgeError(String(e));
    } finally {
      setChecking(false);
    }
  }

  const statusColor =
    health?.status === "ok"
      ? "text-emerald-600"
      : health?.status === "degraded"
        ? "text-amber-600"
        : "text-muted";

  const overallColor =
    knowledge?.overall === "fresh"
      ? "text-emerald-600"
      : knowledge?.overall === "stale"
        ? "text-amber-600"
        : "text-red-600";

  return (
    <div className="min-h-screen p-8 max-w-3xl">
      <h1 className="text-2xl font-semibold">AgentTheSpire</h1>
      <p className="text-muted mt-1">
        Rust + Tauri rewrite — version {__APP_VERSION__}
      </p>
      <p className="text-muted text-sm">
        Runtime: {__IS_TAURI__ ? "Tauri desktop (workstation)" : "Web browser"}
      </p>

      <section className="mt-8 rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Health</h2>
        {healthError && <p className="text-red-500">Error: {healthError}</p>}
        {!healthError && !health && <p className="text-muted">Loading…</p>}
        {health && (
          <>
            <p className="mb-3">
              <span className="text-muted text-sm">Status: </span>
              <span className={`font-medium ${statusColor}`}>
                {health.status}
              </span>
              <span className="text-muted text-sm ml-4">Role: </span>
              <span className="font-medium">{health.role}</span>
              <span className="text-muted text-sm ml-4">Core: </span>
              <span className="font-medium">{health.coreVersion}</span>
            </p>

            <h3 className="text-sm font-medium text-muted mb-1">Config</h3>
            <ul className="text-sm space-y-1 mb-3">
              <li>
                <span className="text-muted">Path: </span>
                <code className="text-xs">{health.config.path ?? "<none>"}</code>
              </li>
              <li>
                <span className="text-muted">File present: </span>
                <span>{String(health.config.filePresent)}</span>
              </li>
              <li>
                <span className="text-muted">Loaded: </span>
                <span>{String(health.config.loaded)}</span>
              </li>
            </ul>
            {health.config.errors.length > 0 && (
              <div className="rounded border border-amber-500/40 bg-amber-50/40 p-2">
                <p className="text-amber-700 text-sm font-medium mb-1">
                  Config issues
                </p>
                <ul className="text-xs space-y-0.5 text-amber-800">
                  {health.config.errors.map((e, i) => (
                    <li key={i}>• {e}</li>
                  ))}
                </ul>
              </div>
            )}
          </>
        )}
      </section>

      <section className="mt-6 rounded border border-muted/30 p-4">
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
        {knowledgeError && (
          <p className="text-red-500">Error: {knowledgeError}</p>
        )}
        {!knowledgeError && !knowledge && (
          <p className="text-muted">Loading…</p>
        )}
        {knowledge && (
          <>
            <p className="mb-3">
              <span className="text-muted text-sm">Overall: </span>
              <span className={`font-medium ${overallColor}`}>
                {knowledge.overall}
              </span>
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
                <p className="text-amber-700 text-sm font-medium mb-1">
                  Warnings
                </p>
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
    </div>
  );
}
