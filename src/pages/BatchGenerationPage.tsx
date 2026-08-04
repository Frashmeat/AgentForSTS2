import { useEffect, useMemo, useState } from "react";
import { BookOpen, Hammer, Package, Play, RotateCcw } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, CardSection, Field, Notice, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import { waitForRun } from "@/services/runPolling";
import type {
  CurrentProject,
  ItemCapabilityCatalog,
  ProjectPackageRequest,
  RunRecord,
  StoredItemDefinition,
} from "@/services/tauriApi";
import {
  buildBatchRequest,
  buildComplexRequest,
  decodeGenerationComposition,
  failedRequestItems,
  type GenerationComposition,
  unprocessedRequestItems,
} from "./batchGenerationModel";
import { capabilityReason, localizedLabel } from "./itemEditorModel";

type Mode = "batch" | "complex" | "build" | "package";

export function BatchGenerationPage() {
  const [mode, setMode] = useState<Mode>("batch");
  const [project, setProject] = useState<CurrentProject | null>(null);
  const [catalog, setCatalog] = useState<ItemCapabilityCatalog | null>(null);
  const [definitions, setDefinitions] = useState<StoredItemDefinition[]>([]);
  const [selectedHashes, setSelectedHashes] = useState<string[]>([]);
  const [failFast, setFailFast] = useState(false);
  const [artifactId, setArtifactId] = useState("mod-package");
  const [modId, setModId] = useState("");
  const [sourceRoot, setSourceRoot] = useState("delivery");
  const [outputPath, setOutputPath] = useState("packages/mod.zip");
  const [run, setRun] = useState<RunRecord | null>(null);
  const [composition, setComposition] = useState<GenerationComposition | null>(null);
  const [childRuns, setChildRuns] = useState<RunRecord[]>([]);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);

  const selectedDefinitions = useMemo(
    () => definitions.filter((item) => selectedHashes.includes(item.definitionHash)),
    [definitions, selectedHashes],
  );
  const batchRequest = useMemo(
    () => project ? buildBatchRequest(project.csharpName, selectedDefinitions, failFast) : null,
    [project, selectedDefinitions, failFast],
  );
  const packageRequest = useMemo<ProjectPackageRequest>(() => ({
    artifactId,
    modId: mode === "complex" && project ? project.csharpName : modId,
    sourceRelativeRoot: sourceRoot,
    outputRelativePath: outputPath,
  }), [artifactId, modId, mode, outputPath, project, sourceRoot]);
  const requestPreview = mode === "batch"
    ? batchRequest
    : mode === "complex" && batchRequest
      ? buildComplexRequest(batchRequest, packageRequest)
      : mode === "package"
        ? packageRequest
        : {};

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void loadWorkspace();
  }, []);

  async function loadWorkspace() {
    setFailure(null);
    try {
      const current = await api.currentProject();
      setProject(current);
      if (!current) {
        setCatalog(null);
        setDefinitions([]);
        setSelectedHashes([]);
        return;
      }
      setModId((value) => value || current.csharpName);
      setArtifactId((value) => value === "mod-package" ? `${current.csharpName}-package` : value);
      setOutputPath((value) => value === "packages/mod.zip" ? `packages/${current.csharpName}.zip` : value);
      const [capabilities, items] = await Promise.all([
        api.getItemCapabilities(),
        api.listItemDefinitions(),
      ]);
      setCatalog(capabilities);
      setDefinitions(items);
      setSelectedHashes((selected) => selected.filter((hash) =>
        items.some((item) => item.definitionHash === hash),
      ));
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    }
  }

  function toggleDefinition(definition: StoredItemDefinition) {
    setSelectedHashes((selected) => selected.includes(definition.definitionHash)
      ? selected.filter((hash) => hash !== definition.definitionHash)
      : [...selected, definition.definitionHash]);
  }

  async function submit() {
    if ((mode === "batch" || mode === "complex") && !batchRequest) return;
    setBusy(true);
    setFailure(null);
    setComposition(null);
    setChildRuns([]);
    try {
      let runId: string;
      if (mode === "batch") {
        if (!batchRequest) return;
        runId = await api.submitBatchGenerate(batchRequest);
      } else if (mode === "complex") {
        if (!batchRequest) return;
        runId = await api.submitComplexGenerate(buildComplexRequest(batchRequest, packageRequest));
      } else if (mode === "build") {
        runId = await api.submitProjectBuild({});
      } else {
        runId = await api.submitProjectPackage(packageRequest);
      }
      await finishRun(runId);
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function retryFailed() {
    if (!composition) return;
    const items = failedRequestItems(composition);
    if (items.length === 0) return;
    setBusy(true);
    setFailure(null);
    try {
      const runId = await api.submitBatchGenerate({
        modId: composition.request.modId,
        items,
        failFast: false,
      });
      await finishRun(runId);
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function finishRun(runId: string) {
    const terminal = await waitForRun(runId, setRun);
    const decoded = decodeGenerationComposition(terminal);
    if ((terminal.featureId === "mod.generate.batch" || terminal.featureId === "mod.generate.complex") && terminal.status === "succeeded" && !decoded) {
      throw toActionableFailure(undefined);
    }
    setComposition(decoded);
    setChildRuns(decoded ? await Promise.all(decoded.childRunIds.map((id) => api.getRun(id))) : []);
  }

  const selectionInvalid = selectedDefinitions.length === 0 || selectedDefinitions.some((item) => {
    const capability = catalog?.itemTypes.find((entry) => entry.descriptor.id === item.definition.itemType);
    return !capability?.ready || item.definition.behaviorIntent.length === 0;
  });
  const packageInvalid = !packageRequest.artifactId.trim() || !packageRequest.modId.trim() ||
    !packageRequest.sourceRelativeRoot.trim() || !packageRequest.outputRelativePath.trim();
  const submitDisabled = busy || !project ||
    ((mode === "batch" || mode === "complex") && selectionInvalid) ||
    ((mode === "complex" || mode === "package") && packageInvalid);

  return (
    <div className="space-y-4">
      <PageHero
        eyebrow="feature · composition and delivery"
        title="Batch and delivery"
        subtitle={project?.name ?? "No project open"}
        actions={<Button onClick={() => void loadWorkspace()}><BookOpen size={14} /> Refresh</Button>}
      />
      {failure && <div data-testid="batch-error"><ActionableErrorNotice failure={failure} /></div>}
      {!project && <Notice variant="warn" title="Project required">Open a project from Dashboard.</Notice>}

      <Card eyebrow="mode" title="Feature">
        <div className="flex gap-2 flex-wrap">
          {(["batch", "complex", "build", "package"] as Mode[]).map((value) => (
            <Button data-testid={`batch-mode-${value}`} key={value} size="sm" variant={mode === value ? "accent" : "ghost"} onClick={() => setMode(value)}>
              {value}
            </Button>
          ))}
        </div>
      </Card>

      {(mode === "batch" || mode === "complex") && (
        <Card
          eyebrow="item library"
          title="Definitions"
          subtitle={`${selectedDefinitions.length} selected · ${catalog?.gamePackId ?? "no Pack"}`}
        >
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2">
            {definitions.map((item) => {
              const capability = catalog?.itemTypes.find((entry) => entry.descriptor.id === item.definition.itemType);
              const ready = capability?.ready === true && item.definition.behaviorIntent.length > 0;
              return (
                <label data-testid="batch-definition" data-item-id={item.definition.itemId} key={item.definitionHash} className={`border rounded-sm p-3 flex gap-3 items-start ${ready ? "border-rule-soft" : "border-rule-soft opacity-60"}`}>
                  <input
                    type="checkbox"
                    checked={selectedHashes.includes(item.definitionHash)}
                    disabled={!ready || busy}
                    onChange={() => toggleDefinition(item)}
                  />
                  <span className="min-w-0">
                    <span className="block font-mono text-sm break-all">{item.definition.itemId}</span>
                    <span className="block text-xs text-ink-mute mt-1">
                      {capability ? localizedLabel(capability.descriptor.displayNames) : item.definition.itemType} · {item.definitionHash.slice(0, 12)}
                    </span>
                    {!ready && <span className="block text-xs text-status-warn mt-1">
                      {item.definition.behaviorIntent.length === 0 ? "Behavior intent required" : capability ? capabilityReason(capability) : "Type unavailable"}
                    </span>}
                  </span>
                </label>
              );
            })}
          </div>
          {definitions.length === 0 && <Notice variant="muted" title="No saved definitions" />}
          <div className="mt-3">
            <label className="inline-flex items-center gap-2 text-sm">
              <input type="checkbox" checked={failFast} onChange={(event) => setFailFast(event.target.checked)} />
              Stop after first failed item
            </label>
          </div>
        </Card>
      )}

      {(mode === "complex" || mode === "package") && (
        <Card eyebrow="delivery" title="Package">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <Field label="Artifact ID"><input className="input-mono" value={artifactId} onChange={(event) => setArtifactId(event.target.value)} /></Field>
            <Field label="Mod ID"><input className="input-mono" value={packageRequest.modId} disabled={mode === "complex"} onChange={(event) => setModId(event.target.value)} /></Field>
            <Field label="Source root"><input className="input-mono" value={sourceRoot} onChange={(event) => setSourceRoot(event.target.value)} /></Field>
            <Field label="Output path"><input className="input-mono" value={outputPath} onChange={(event) => setOutputPath(event.target.value)} /></Field>
          </div>
        </Card>
      )}

      <Card
        eyebrow="typed request"
        title="Request preview"
        actions={
          <Button data-testid="batch-submit" variant="success" disabled={submitDisabled} onClick={() => void submit()}>
            {mode === "build" ? <Hammer size={15} /> : mode === "package" ? <Package size={15} /> : <Play size={15} />}
            Run
          </Button>
        }
      >
        <pre data-testid="batch-request-preview" className="pre-block max-h-80">{JSON.stringify(requestPreview, null, 2)}</pre>
      </Card>

      {run && <RunResult run={run} composition={composition} childRuns={childRuns} busy={busy} onRetry={() => void retryFailed()} />}
    </div>
  );
}

function RunResult({
  run,
  composition,
  childRuns,
  busy,
  onRetry,
}: {
  run: RunRecord;
  composition: GenerationComposition | null;
  childRuns: RunRecord[];
  busy: boolean;
  onRetry: () => void;
}) {
  const failed = composition ? failedRequestItems(composition) : [];
  const unprocessed = composition ? unprocessedRequestItems(composition) : [];
  return (
    <div data-testid="batch-run-result" data-run-id={run.id} data-run-status={run.status}>
      <Card
        eyebrow="run"
        title={run.featureId}
        actions={<Badge variant={run.status === "succeeded" ? "ok" : run.status === "failed" ? "error" : "warn"}>{run.status}</Badge>}
      >
      {run.failure && <Notice variant="error" title={run.failure.code}>{run.failure.stage}</Notice>}
      {composition?.deliverySkipped && <Notice variant="warn" title="Delivery skipped">One or more items did not succeed.</Notice>}
      {unprocessed.length > 0 && <Notice variant="muted" title={`${unprocessed.length} items not run`}>{unprocessed.map((item) => item.definition.definition.itemId).join(", ")}</Notice>}
      {failed.length > 0 && (
        <div className="mt-3">
          <Button variant="accent" disabled={busy} onClick={onRetry}><RotateCcw size={14} /> Retry failed items</Button>
        </div>
      )}
      {childRuns.length > 0 && (
        <CardSection title="Persisted child Runs">
          <div className="divide-y divide-rule-soft border border-rule-soft rounded-sm">
            {childRuns.map((child) => (
              <div data-testid="batch-child-run" data-run-id={child.id} data-run-status={child.status} key={child.id} className="p-3 flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <span className="block text-sm font-medium">{child.featureId}</span>
                  <span className="block font-mono text-xs text-ink-mute truncate">{child.id}</span>
                  {child.failure && <span className="block font-mono text-xs text-status-error mt-1">{child.failure.code}</span>}
                </div>
                <Badge variant={child.status === "succeeded" ? "ok" : child.status === "failed" ? "error" : "warn"}>{child.status}</Badge>
              </div>
            ))}
          </div>
        </CardSection>
      )}
        {run.result && <pre data-testid="batch-run-payload" className="pre-block max-h-96 mt-3">{JSON.stringify(run.result.payload, null, 2)}</pre>}
      </Card>
    </div>
  );
}
