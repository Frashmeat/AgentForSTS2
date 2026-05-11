import { useEffect, useState } from "react";
import { api } from "@/services/api";
import type { HealthReport } from "@/services/tauriApi";

export default function App() {
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (api.getHealth() as Promise<HealthReport>)
      .then(setHealth)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <div className="min-h-screen p-8">
      <h1 className="text-2xl font-semibold">AgentTheSpire</h1>
      <p className="text-muted mt-1">
        Rust + Tauri rewrite — version {__APP_VERSION__}
      </p>
      <p className="text-muted text-sm">
        Runtime: {__IS_TAURI__ ? "Tauri desktop (workstation)" : "Web browser"}
      </p>

      <section className="mt-8 rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Health</h2>
        {error && <p className="text-red-500">Error: {error}</p>}
        {!error && !health && <p className="text-muted">Loading…</p>}
        {health && (
          <pre className="text-sm whitespace-pre-wrap">
            {JSON.stringify(health, null, 2)}
          </pre>
        )}
      </section>
    </div>
  );
}
