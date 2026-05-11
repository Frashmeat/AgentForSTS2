import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { HealthReport } from "@/services/tauriApi";

export function HealthCard() {
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (api.getHealth() as Promise<HealthReport>)
      .then(setHealth)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  const statusColor =
    health?.status === "ok"
      ? "text-emerald-600"
      : health?.status === "degraded"
        ? "text-amber-600"
        : "text-muted";

  return (
    <section className="rounded border border-muted/30 p-4">
      <h2 className="text-lg font-medium mb-2">Health</h2>
      {error && <p className="text-red-500">Error: {error}</p>}
      {!error && !health && <p className="text-muted">Loading…</p>}
      {health && (
        <>
          <p className="mb-3">
            <span className="text-muted text-sm">Status: </span>
            <span className={`font-medium ${statusColor}`}>{health.status}</span>
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

          <div className="mt-3 pt-3 border-t border-muted/20">
            <h3 className="text-sm font-medium text-muted mb-1">Readiness</h3>
            <ul className="text-sm space-y-0.5">
              <li>
                <span
                  className={
                    health.readiness.llmConfigured ? "text-emerald-600" : "text-amber-600"
                  }
                >
                  {health.readiness.llmConfigured ? "✓" : "○"}
                </span>{" "}
                LLM api_key 配置
              </li>
              <li>
                <span
                  className={
                    health.readiness.imageGenConfigured
                      ? "text-emerald-600"
                      : "text-muted"
                  }
                >
                  {health.readiness.imageGenConfigured ? "✓" : "○"}
                </span>{" "}
                image_gen api_key 配置（asset_generate 需要）
              </li>
              <li>
                <span
                  className={
                    health.readiness.activeProjectOpen
                      ? "text-emerald-600"
                      : "text-muted"
                  }
                >
                  {health.readiness.activeProjectOpen ? "✓" : "○"}
                </span>{" "}
                工程文件夹已打开
              </li>
            </ul>
          </div>
        </>
      )}
    </section>
  );
}
