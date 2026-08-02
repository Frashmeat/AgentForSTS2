import { useState } from "react";
import { Hammer, Package, Play } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, Field, Notice, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import { waitForRun } from "@/services/runPolling";
import type {
  BatchGenerateRequest,
  ComplexGenerateRequest,
  ProjectPackageRequest,
  RunRecord,
} from "@/services/tauriApi";

type Mode = "batch" | "complex" | "build" | "package";

export function BatchGenerationPage() {
  const [mode, setMode] = useState<Mode>("batch");
  const [requestJson, setRequestJson] = useState('{\n  "items": [],\n  "failFast": true\n}');
  const [artifactId, setArtifactId] = useState("mod-package");
  const [modId, setModId] = useState("");
  const [sourceRoot, setSourceRoot] = useState("delivery");
  const [outputPath, setOutputPath] = useState("packages/mod.zip");
  const [run, setRun] = useState<RunRecord | null>(null);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true); setFailure(null);
    try {
      let runId: string;
      if (mode === "batch") {
        runId = await api.submitBatchGenerate(JSON.parse(requestJson) as BatchGenerateRequest) as string;
      } else if (mode === "complex") {
        runId = await api.submitComplexGenerate(JSON.parse(requestJson) as ComplexGenerateRequest) as string;
      } else if (mode === "build") {
        runId = await api.submitProjectBuild({}) as string;
      } else {
        const request: ProjectPackageRequest = {
          artifactId, modId, sourceRelativeRoot: sourceRoot, outputRelativePath: outputPath,
        };
        runId = await api.submitProjectPackage(request) as string;
      }
      await waitForRun(runId, setRun);
    } catch (error: unknown) { setFailure(toActionableFailure(error)); }
    finally { setBusy(false); }
  }

  return (
    <div className="space-y-4">
      <PageHero eyebrow="feature · composition and delivery" title="Batch and delivery" subtitle="Typed Stage 2 execution" />
      <ActionableErrorNotice failure={failure} />
      <Card eyebrow="mode" title="Feature">
        <div className="flex gap-2 flex-wrap mb-4">
          {(["batch", "complex", "build", "package"] as Mode[]).map((value) => (
            <Button key={value} size="sm" variant={mode === value ? "accent" : "ghost"} onClick={() => setMode(value)}>{value}</Button>
          ))}
        </div>
        {(mode === "batch" || mode === "complex") && (
          <Field label="Request"><textarea className="input-mono min-h-80" value={requestJson} onChange={(event) => setRequestJson(event.target.value)} /></Field>
        )}
        {mode === "package" && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <Field label="Artifact ID"><input className="input-mono" value={artifactId} onChange={(event) => setArtifactId(event.target.value)} /></Field>
            <Field label="Mod ID"><input className="input-mono" value={modId} onChange={(event) => setModId(event.target.value)} /></Field>
            <Field label="Source root"><input className="input-mono" value={sourceRoot} onChange={(event) => setSourceRoot(event.target.value)} /></Field>
            <Field label="Output path"><input className="input-mono" value={outputPath} onChange={(event) => setOutputPath(event.target.value)} /></Field>
          </div>
        )}
        <div className="mt-4">
          <Button variant="success" disabled={busy} onClick={() => void submit()}>
            {mode === "build" ? <Hammer size={15} /> : mode === "package" ? <Package size={15} /> : <Play size={15} />}
            Run
          </Button>
        </div>
      </Card>
      {run && (
        <Card eyebrow="run" title={run.featureId} actions={<Badge variant={run.status === "succeeded" ? "ok" : run.status === "failed" ? "error" : "warn"}>{run.status}</Badge>}>
          {run.failure && <Notice variant="error" title={run.failure.code}>{run.failure.stage}</Notice>}
          {run.result && <pre className="code-block max-h-96 overflow-auto">{JSON.stringify(run.result.payload, null, 2)}</pre>}
        </Card>
      )}
    </div>
  );
}
