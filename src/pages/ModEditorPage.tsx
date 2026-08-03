import { useEffect, useState } from "react";
import { Play, WandSparkles } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, Field, Notice, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import { waitForRun } from "@/services/runPolling";
import type { CurrentProject, PlanItem, RunRecord, SelectedResource } from "@/services/tauriApi";

export function ModEditorPage() {
  const [project, setProject] = useState<CurrentProject | null>(null);
  const [requirements, setRequirements] = useState("");
  const [itemType, setItemType] = useState("custom_code");
  const [artifactId, setArtifactId] = useState("mod-item");
  const [resourcesJson, setResourcesJson] = useState("[]");
  const [plan, setPlan] = useState<PlanItem | null>(null);
  const [run, setRun] = useState<RunRecord | null>(null);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (__IS_TAURI__) void api.currentProject().then((value) => setProject(value as CurrentProject | null));
  }, []);

  async function planMod() {
    setBusy(true); setFailure(null); setPlan(null);
    try {
      const id = await api.submitModPlan({ requirements, itemType }) as string;
      const terminal = await waitForRun(id, setRun);
      if (terminal.status === "succeeded") {
        const value = decodePlan(terminal);
        if (value) setPlan(value);
      }
    } catch (error: unknown) { setFailure(toActionableFailure(error)); }
    finally { setBusy(false); }
  }

  async function generate() {
    if (!plan || !project) return;
    setBusy(true); setFailure(null);
    try {
      const selectedResources = JSON.parse(resourcesJson) as SelectedResource[];
      if (!Array.isArray(selectedResources)) throw new Error("invalid resources");
      const id = await api.submitSingleGenerate({
        artifactId,
        modId: project.csharpName,
        plan,
        selectedResources,
      }) as string;
      await waitForRun(id, setRun);
    } catch (error: unknown) { setFailure(toActionableFailure(error)); }
    finally { setBusy(false); }
  }

  return (
    <div className="space-y-4">
      <PageHero eyebrow="feature · mod.plan + mod.generate.single" title="Mod generation" subtitle={project?.name ?? "No project open"} />
      <ActionableErrorNotice failure={failure} />
      {!project && <Notice variant="warn" title="Project required">Open a project from Dashboard.</Notice>}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card eyebrow="plan" title="Design contract">
          <Field label="Item type">
            <select className="input-mono" value={itemType} onChange={(event) => setItemType(event.target.value)}>
              <option value="custom_code">custom_code</option>
              <option value="relic">relic</option>
              <option value="card">card</option>
              <option value="power">power</option>
              <option value="character">character</option>
            </select>
          </Field>
          <Field label="Requirements"><textarea className="input-mono min-h-44" value={requirements} onChange={(event) => setRequirements(event.target.value)} /></Field>
          <Button variant="accent" disabled={busy || !project || !requirements.trim()} onClick={() => void planMod()}>
            <WandSparkles size={15} /> Plan
          </Button>
        </Card>
        <Card eyebrow="generate" title={plan?.name ?? "Awaiting plan"}>
          {plan && <pre className="code-block max-h-64 overflow-auto">{JSON.stringify(plan, null, 2)}</pre>}
          <Field label="Artifact ID"><input className="input-mono" value={artifactId} onChange={(event) => setArtifactId(event.target.value)} /></Field>
          <Field label="Selected resources"><textarea className="input-mono min-h-24" value={resourcesJson} onChange={(event) => setResourcesJson(event.target.value)} /></Field>
          <Button variant="success" disabled={busy || !plan || !artifactId.trim()} onClick={() => void generate()}>
            <Play size={15} /> Generate
          </Button>
        </Card>
      </div>
      {run && <RunResult run={run} />}
    </div>
  );
}

function decodePlan(run: RunRecord): PlanItem | null {
  if (run.result?.schema.id !== "feature.mod-plan-result" || run.result.schema.version !== 2) {
    return null;
  }
  const value = run.result.payload;
  return isPlanItem(value) ? value : null;
}

function isPlanItem(value: Record<string, unknown>): value is PlanItem {
  return (
    typeof value.itemId === "string" &&
    typeof value.itemType === "string" &&
    typeof value.name === "string" &&
    typeof value.summary === "string" &&
    isStringArray(value.behaviorIntent) &&
    isStringArray(value.implementationConstraints) &&
    isStringArray(value.evidenceRequirements) &&
    isStringArray(value.requiredResourceRoles) &&
    isStringArray(value.acceptanceCriteria)
  );
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function RunResult({ run }: { run: RunRecord }) {
  return (
    <Card eyebrow="run" title={run.featureId} actions={<Badge variant={run.status === "succeeded" ? "ok" : run.status === "failed" ? "error" : "warn"}>{run.status}</Badge>}>
      {run.failure && <Notice variant="error" title={run.failure.code}>{run.failure.stage}</Notice>}
      {run.result && <pre className="code-block max-h-80 overflow-auto">{JSON.stringify(run.result.payload, null, 2)}</pre>}
    </Card>
  );
}
