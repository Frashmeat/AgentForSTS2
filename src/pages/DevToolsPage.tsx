import { useEffect, useState } from "react";
import { Ban, RefreshCw } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, Notice, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import type { FeatureContract, RunRecord, RunSummary } from "@/services/tauriApi";

export function DevToolsPage() {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [selected, setSelected] = useState<RunRecord | null>(null);
  const [features, setFeatures] = useState<FeatureContract[]>([]);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);

  async function reload() {
    setFailure(null);
    try {
      const catalog = await api.getFeatureCatalog() as FeatureContract[];
      setFeatures(catalog);
      if (__IS_TAURI__) setRuns(await api.listRuns() as RunSummary[]);
    } catch (error: unknown) { setFailure(toActionableFailure(error)); }
  }

  useEffect(() => { void reload(); }, []);
  useEffect(() => {
    if (!runs.some((run) => run.status === "pending" || run.status === "running")) return;
    const timer = window.setInterval(() => { void reload(); }, 700);
    return () => window.clearInterval(timer);
  }, [runs]);

  async function inspect(id: string) {
    try { setSelected(await api.getRun(id) as RunRecord); }
    catch (error: unknown) { setFailure(toActionableFailure(error)); }
  }

  return (
    <div className="space-y-4">
      <PageHero eyebrow="runtime · schema v3" title="Runs" subtitle={`${features.length} registered Features`} actions={
        <Button size="sm" onClick={() => void reload()}><RefreshCw size={15} /> Refresh</Button>
      } />
      <ActionableErrorNotice failure={failure} />
      <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] gap-4">
        <Card eyebrow="history" title="Run repository">
          <div className="space-y-2">
            {runs.map((run) => (
              <button key={run.id} type="button" className="w-full topbtn ghost text-left" onClick={() => void inspect(run.id)}>
                <div className="flex justify-between gap-3"><strong>{run.featureId}</strong><Badge variant={run.status === "succeeded" ? "ok" : run.status === "failed" ? "error" : "warn"}>{run.status}</Badge></div>
                <code>{run.id}</code>
              </button>
            ))}
            {runs.length === 0 && <p>No v3 Runs.</p>}
          </div>
        </Card>
        <Card eyebrow="detail" title={selected?.featureId ?? "Select a Run"} actions={selected && (selected.status === "pending" || selected.status === "running") && (
          <Button size="sm" variant="danger" onClick={() => void api.cancelRun(selected.id).then(() => reload())}><Ban size={15} /> Cancel</Button>
        )}>
          {selected?.failure && <Notice variant="error" title={selected.failure.code}>{selected.failure.stage}</Notice>}
          {selected && <pre className="code-block max-h-[38rem] overflow-auto">{JSON.stringify(selected, null, 2)}</pre>}
        </Card>
      </div>
      <Card eyebrow="catalog" title="Feature contracts">
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {features.map((feature) => (
            <div key={feature.id} className="card-section">
              <strong>{feature.id}</strong>
              <div><code>{feature.requestSchema.id}@{feature.requestSchema.version}</code></div>
              <div><code>{feature.resultSchema.id}@{feature.resultSchema.version}</code></div>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
