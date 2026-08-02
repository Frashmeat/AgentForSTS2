import { useState } from "react";
import { Search } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, Field, Notice, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import { waitForRun } from "@/services/runPolling";
import type { RunRecord } from "@/services/tauriApi";

export function LogAnalysisPage() {
  const [logText, setLogText] = useState("");
  const [contextHint, setContextHint] = useState("");
  const [run, setRun] = useState<RunRecord | null>(null);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);

  async function analyze() {
    setBusy(true); setFailure(null);
    try {
      const id = await api.submitLogAnalyze({ logText, contextHint: contextHint || null }) as string;
      await waitForRun(id, setRun);
    } catch (error: unknown) { setFailure(toActionableFailure(error)); }
    finally { setBusy(false); }
  }

  return (
    <div className="space-y-4">
      <PageHero eyebrow="feature · log.analyze" title="Log analysis" subtitle="Pack rules and verified Truth evidence" />
      <ActionableErrorNotice failure={failure} />
      <Card eyebrow="input" title="Runtime log">
        <Field label="Context"><input className="input-mono" value={contextHint} onChange={(event) => setContextHint(event.target.value)} /></Field>
        <Field label="Log"><textarea className="input-mono min-h-80" value={logText} onChange={(event) => setLogText(event.target.value)} /></Field>
        <Button variant="accent" disabled={busy || !logText.trim()} onClick={() => void analyze()}>
          <Search size={15} /> Analyze
        </Button>
      </Card>
      {run && (
        <Card eyebrow="run" title={run.featureId} actions={<Badge variant={run.status === "succeeded" ? "ok" : run.status === "failed" ? "error" : "warn"}>{run.status}</Badge>}>
          {run.failure && <Notice variant="error" title={run.failure.code}>{run.failure.stage}</Notice>}
          {run.result && <pre className="code-block max-h-[34rem] overflow-auto">{JSON.stringify(run.result.payload, null, 2)}</pre>}
        </Card>
      )}
    </div>
  );
}
