import { useEffect, useMemo, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  FileCheck2,
  Hammer,
  ListFilter,
  RefreshCw,
  Save,
  Sparkles,
  Trash2,
} from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, CardSection, Field, Notice, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import { waitForRun } from "@/services/runPolling";
import type {
  CompositionDraft,
  CompositionProfileSet,
  ItemCapabilityCatalog,
  RunRecord,
  StoredItemDefinition,
} from "@/services/tauriApi";
import {
  buildCompositionPlanRequest,
  buildCompositionRetryNodeRequest,
  buildCompositionGenerateRequest,
  closedSelection,
  compositionRoots,
  defaultProfileChoice,
  displayName,
  draftRows,
  pageRows,
  parametersForChoice,
  profileIssues,
  type CompositionProfileChoice,
  type DraftNodeStatus,
} from "./compositionStudioModel";

const PAGE_SIZE = 12;

export function CompositionStudioPage() {
  const [catalog, setCatalog] = useState<ItemCapabilityCatalog | null>(null);
  const [projectOpen, setProjectOpen] = useState(false);
  const [drafts, setDrafts] = useState<CompositionDraft[]>([]);
  const [definitions, setDefinitions] = useState<StoredItemDefinition[]>([]);
  const [compositionId, setCompositionId] = useState("");
  const [choice, setChoice] = useState<CompositionProfileChoice | null>(null);
  const [parameters, setParameters] = useState<Record<string, number>>({});
  const [draftId, setDraftId] = useState("");
  const [concept, setConcept] = useState("");
  const [selectedDraftId, setSelectedDraftId] = useState("");
  const [selectedNodeId, setSelectedNodeId] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [itemType, setItemType] = useState("");
  const [status, setStatus] = useState<"all" | DraftNodeStatus>("all");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);
  const [selectedRootKey, setSelectedRootKey] = useState("");
  const [artifactId, setArtifactId] = useState("");
  const [modId, setModId] = useState("");
  const [sourceRoot, setSourceRoot] = useState("delivery");
  const [outputPath, setOutputPath] = useState("packages/mod.zip");
  const [lastGenerationRunId, setLastGenerationRunId] = useState("");
  const [lastGenerationRun, setLastGenerationRun] = useState<RunRecord | null>(null);
  const [retryInstructions, setRetryInstructions] = useState("");
  const [lastRetryRun, setLastRetryRun] = useState<RunRecord | null>(null);

  const profile = useMemo(
    () => catalog?.compositionProfiles.find((value) => value.id === compositionId) ?? null,
    [catalog, compositionId],
  );
  const draft = useMemo(
    () => drafts.find((value) => value.draftId === selectedDraftId) ?? null,
    [drafts, selectedDraftId],
  );
  const rows = useMemo(
    () => draft ? draftRows(draft, itemType, status, query) : [],
    [draft, itemType, status, query],
  );
  const paged = useMemo(() => pageRows(rows, page, PAGE_SIZE), [rows, page]);
  const selectedNode = draft && selectedNodeId ? draft.nodes[selectedNodeId] : null;
  const issues = profile ? profileIssues(profile, parameters) : [];
  const itemTypes = useMemo(
    () => Array.from(new Set(Object.values(draft?.nodes ?? {}).map((node) => node.definition.itemType))).sort(),
    [draft],
  );
  const roots = useMemo(
    () => compositionRoots(definitions, catalog?.compositionProfiles ?? []),
    [definitions, catalog],
  );
  const selectedRoot = roots.find((value) => definitionKey(value) === selectedRootKey) ?? roots[0] ?? null;

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void load();
  }, []);

  async function load(preferredDraftId?: string) {
    setFailure(null);
    try {
      const nextCatalog = await api.getItemCapabilities();
      const current = await api.currentProject();
      const [nextDrafts, nextDefinitions] = current
        ? await Promise.all([api.listCompositionDrafts(), api.listItemDefinitions()])
        : [[], []];
      setCatalog(nextCatalog);
      setProjectOpen(current !== null);
      setDrafts(nextDrafts);
      setDefinitions(nextDefinitions);
      setSelectedRootKey((value) => {
        const nextRoots = compositionRoots(nextDefinitions, nextCatalog.compositionProfiles);
        return nextRoots.some((definition) => definitionKey(definition) === value)
          ? value
          : nextRoots[0] ? definitionKey(nextRoots[0]) : "";
      });
      if (current) {
        setModId((value) => value || current.csharpName);
        setArtifactId((value) => value || `${current.csharpName}-composition`);
        setOutputPath((value) => value === "packages/mod.zip" ? `packages/${current.csharpName}.zip` : value);
      }
      const nextProfile = nextCatalog.compositionProfiles.find((value) => value.id === compositionId)
        ?? nextCatalog.compositionProfiles[0];
      if (nextProfile && (!profile || profile.id !== nextProfile.id)) selectComposition(nextProfile);
      const nextDraftId = preferredDraftId || selectedDraftId || nextDrafts[0]?.draftId || "";
      setSelectedDraftId(nextDrafts.some((value) => value.draftId === nextDraftId) ? nextDraftId : nextDrafts[0]?.draftId ?? "");
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    }
  }

  function selectComposition(next: CompositionProfileSet) {
    const nextChoice = defaultProfileChoice(next);
    setCompositionId(next.id);
    setChoice(nextChoice);
    setParameters(parametersForChoice(next, nextChoice));
  }

  function selectProfile(nextChoice: CompositionProfileChoice) {
    if (!profile) return;
    setChoice(nextChoice);
    setParameters(parametersForChoice(profile, nextChoice));
  }

  async function planComposition() {
    if (!profile || !choice || issues.length > 0) return;
    setBusy(true);
    setFailure(null);
    try {
      const runId = await api.submitCompositionPlan(
        buildCompositionPlanRequest(draftId, profile, choice, parameters, concept),
      );
      const terminal = await waitForRun(runId);
      if (terminal.status === "succeeded") await load(draftId.trim());
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function persistNodes(nodes: CompositionDraft["nodes"]) {
    if (!draft) return;
    setBusy(true);
    setFailure(null);
    try {
      const updated = await api.updateCompositionDraft(draft.draftId, draft.revision, nodes);
      setDrafts((values) => values.map((value) => value.draftId === updated.draftId ? updated : value));
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function removeNode(itemId: string) {
    if (!draft || itemId === draft.rootItemId) return;
    const nodes = { ...draft.nodes };
    delete nodes[itemId];
    setSelectedIds((values) => new Set(Array.from(values).filter((value) => value !== itemId)));
    setSelectedNodeId("");
    await persistNodes(nodes);
  }

  async function replaceNode() {
    if (!draft || !selectedNode) return;
    const replacement = definitions.find(
      (value) => value.definition.itemId === selectedNode.definition.itemId
        && value.definition.itemType === selectedNode.definition.itemType,
    );
    if (!replacement) return;
    await persistNodes({
      ...draft.nodes,
      [selectedNode.definition.itemId]: {
        definition: replacement.definition,
        expectedCurrentDefinitionHash: replacement.definitionHash,
      },
    });
  }

  async function saveBehavior(value: string) {
    if (!draft || !selectedNode) return;
    const behaviorIntent = value.split("\n").map((entry) => entry.trim()).filter(Boolean);
    await persistNodes({
      ...draft.nodes,
      [selectedNode.definition.itemId]: {
        ...selectedNode,
        definition: { ...selectedNode.definition, behaviorIntent },
      },
    });
  }

  async function retryNode() {
    if (!draft || !selectedNode || !retryInstructions.trim()) return;
    setBusy(true);
    setFailure(null);
    setLastRetryRun(null);
    try {
      const runId = await api.submitCompositionRetryNode(buildCompositionRetryNodeRequest(
        draft,
        selectedNode.definition.itemId,
        retryInstructions,
      ));
      const terminal = await waitForRun(runId, setLastRetryRun);
      if (terminal.status === "succeeded") {
        const itemId = selectedNode.definition.itemId;
        await load(draft.draftId);
        setSelectedNodeId(itemId);
        setRetryInstructions("");
      }
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function confirmSelected() {
    if (!draft || !closedSelection(draft, selectedIds)) return;
    setBusy(true);
    setFailure(null);
    try {
      const confirmation = await api.confirmCompositionDraft(draft.draftId, draft.revision, Array.from(selectedIds));
      const nextDefinitions = await api.listItemDefinitions();
      setDefinitions(nextDefinitions);
      const root = confirmation.definitions.find((value) => value.definition.itemId === draft.rootItemId);
      if (root) setSelectedRootKey(definitionKey(root));
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function generateComposition() {
    if (!selectedRoot || !artifactId.trim() || !modId.trim() || !sourceRoot.trim() || !outputPath.trim()) return;
    setBusy(true);
    setFailure(null);
    try {
      const runId = await api.submitCompositionGenerate(buildCompositionGenerateRequest(
        artifactId,
        modId,
        selectedRoot,
        draft,
        {
          sourceRelativeRoot: sourceRoot,
          outputRelativePath: outputPath,
          compressionLevel: 6,
        },
      ));
      setLastGenerationRunId(runId);
      setLastGenerationRun(null);
      await waitForRun(runId, setLastGenerationRun);
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function deleteDraft() {
    if (!draft || !window.confirm(`Delete Draft ${draft.draftId}?`)) return;
    setBusy(true);
    try {
      await api.deleteCompositionDraft(draft.draftId, draft.revision);
      await load();
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-4">
      <PageHero
        eyebrow="pack-driven composition"
        title="Composition Studio"
        subtitle="Plan, review, and atomically confirm related Mod items."
        actions={<Button data-testid="composition-refresh" onClick={() => void load()}><RefreshCw size={14} /> Refresh</Button>}
      />
      {failure && <div data-testid="composition-error"><ActionableErrorNotice failure={failure} /></div>}
      {!projectOpen && <Notice variant="warn" title="Project required">Open a project before planning or reviewing compositions.</Notice>}
      {catalog && catalog.compositionProfiles.length === 0 && (
        <Notice variant="muted" title="No composition types available">
          The active Game Pack does not currently declare a composition workflow.
        </Notice>
      )}

      {profile && choice && (
        <Card eyebrow="composition plan" title={displayName(profile.displayNames)} subtitle={`Default: ${profile.defaultProfile} · limit ${profile.maxNodes} nodes`}>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-3">
            <Field label="Composition type">
              <select data-testid="composition-type" value={compositionId} onChange={(event) => {
                const next = catalog?.compositionProfiles.find((value) => value.id === event.target.value);
                if (next) selectComposition(next);
              }}>
                {catalog?.compositionProfiles.map((value) => <option key={value.id} value={value.id}>{displayName(value.displayNames)}</option>)}
              </select>
            </Field>
            <Field label="Draft ID"><input data-testid="composition-draft-id" className="input-mono" value={draftId} onChange={(event) => setDraftId(event.target.value)} /></Field>
            <Field label="Profile">
              <select data-testid="composition-profile" value={choice.kind === "preset" ? choice.profileId : "__custom"} onChange={(event) => {
                selectProfile(event.target.value === "__custom"
                  ? { kind: "custom", baseProfileId: profile.customBaseProfile }
                  : { kind: "preset", profileId: event.target.value });
              }}>
                {profile.profiles.map((value) => <option key={value.id} value={value.id}>{displayName(value.displayNames)}</option>)}
                <option value="__custom">Custom</option>
              </select>
            </Field>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mt-3">
            {profile.parameters.map((parameter) => (
              <Field key={parameter.id} label={displayName(parameter.displayNames)} hint={`${parameter.min}–${parameter.max}`}>
                <input data-testid={`composition-parameter-${parameter.id}`} type="number" min={parameter.min} max={parameter.max} disabled={choice.kind !== "custom"} value={parameters[parameter.id] ?? ""} onChange={(event) => setParameters((values) => ({ ...values, [parameter.id]: Number(event.target.value) }))} />
              </Field>
            ))}
          </div>
          <Field label="Concept"><textarea data-testid="composition-concept" className="min-h-28 mt-3" value={concept} onChange={(event) => setConcept(event.target.value)} /></Field>
          {issues.length > 0 && <Notice variant="warn" title="Profile checks">{issues.join(" ")}</Notice>}
          <Button data-testid="composition-plan" variant="accent" disabled={busy || !projectOpen || !draftId.trim() || !concept.trim() || issues.length > 0} onClick={() => void planComposition()}><Sparkles size={14} /> Plan Draft</Button>
        </Card>
      )}

      {catalog && catalog.compositionProfiles.length > 0 && (
        <Card eyebrow="whole-closure publication" title="Generate composition" subtitle="Generate every pinned definition in one isolated build and publish only after the complete closure passes.">
          {roots.length === 0 ? (
            <Notice variant="muted" title="No confirmed root">Confirm a complete composition root before generation.</Notice>
          ) : (
            <>
              <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
                <Field label="Composition root">
                  <select data-testid="composition-generate-root" value={selectedRoot ? definitionKey(selectedRoot) : ""} onChange={(event) => setSelectedRootKey(event.target.value)}>
                    {roots.map((value) => <option key={definitionKey(value)} value={definitionKey(value)}>{value.definition.itemId} · {value.definitionHash.slice(0, 12)}</option>)}
                  </select>
                </Field>
                <Field label="Artifact ID"><input data-testid="composition-artifact-id" className="input-mono" value={artifactId} onChange={(event) => setArtifactId(event.target.value)} /></Field>
                <Field label="Mod ID"><input data-testid="composition-mod-id" className="input-mono" value={modId} onChange={(event) => setModId(event.target.value)} /></Field>
                <Field label="Build output root"><input data-testid="composition-source-root" className="input-mono" value={sourceRoot} onChange={(event) => setSourceRoot(event.target.value)} /></Field>
                <Field label="Package output"><input data-testid="composition-output-path" className="input-mono" value={outputPath} onChange={(event) => setOutputPath(event.target.value)} /></Field>
              </div>
              <div className="flex items-center gap-3 mt-3 flex-wrap">
                <Button data-testid="composition-generate" variant="accent" disabled={busy || !projectOpen || !selectedRoot || !artifactId.trim() || !modId.trim() || !sourceRoot.trim() || !outputPath.trim()} onClick={() => void generateComposition()}><Hammer size={14} /> Generate and package</Button>
                {lastGenerationRunId && <span className="font-mono text-xs text-ink-mute">{lastGenerationRunId}</span>}
              </div>
            </>
          )}
        </Card>
      )}

      {lastGenerationRun && (
        <Card
          eyebrow="composition run"
          title={lastGenerationRun.featureId}
          actions={(
            <Badge variant={lastGenerationRun.status === "succeeded" ? "ok" : lastGenerationRun.status === "failed" ? "error" : "warn"}>
              {lastGenerationRun.status}
            </Badge>
          )}
        >
          {lastGenerationRun.failure && (
            <Notice variant="error" title={lastGenerationRun.failure.code}>
              {lastGenerationRun.failure.stage}
            </Notice>
          )}
        </Card>
      )}

      <div className="grid grid-cols-1 xl:grid-cols-[minmax(260px,0.65fr)_minmax(0,1.9fr)] gap-4">
        <Card eyebrow="draft repository" title="Drafts">
          <div className="space-y-2">
            {drafts.length === 0 && <p className="text-sm text-ink-mute">No saved Drafts.</p>}
            {drafts.map((value) => (
              <button key={value.draftId} data-testid={`composition-draft-${value.draftId}`} type="button" className={`w-full text-left border rounded-sm p-2 ${value.draftId === selectedDraftId ? "border-accent" : "border-rule-soft"}`} onClick={() => { setSelectedDraftId(value.draftId); setSelectedIds(new Set()); setSelectedNodeId(""); setPage(1); }}>
                <span className="block font-mono text-xs">{value.draftId}</span>
                <span className="block text-xs text-ink-mute">r{value.revision} · {Object.keys(value.nodes).length} items</span>
              </button>
            ))}
          </div>
        </Card>

        <Card eyebrow="draft review" title={draft?.draftId ?? "Select a Draft"} subtitle={draft ? `Root ${draft.rootItemId} · revision ${draft.revision}` : "Draft content remains separate from current Item pointers."} actions={draft && <Button data-testid="composition-delete-draft" variant="danger" disabled={busy} onClick={() => void deleteDraft()}><Trash2 size={14} /> Delete Draft</Button>}>
          {draft && (
            <>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                <Field label="Search"><input data-testid="composition-search" value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} /></Field>
                <Field label="Item type"><select data-testid="composition-type-filter" value={itemType} onChange={(event) => { setItemType(event.target.value); setPage(1); }}><option value="">All types</option>{itemTypes.map((value) => <option key={value}>{value}</option>)}</select></Field>
                <Field label="Status"><select data-testid="composition-status-filter" value={status} onChange={(event) => { setStatus(event.target.value as typeof status); setPage(1); }}><option value="all">All states</option><option value="new">New</option><option value="replacement">Replacement</option></select></Field>
              </div>
              <div className="flex items-center justify-between gap-2 mt-3 flex-wrap">
                <div className="flex gap-2">
                  <Button size="sm" onClick={() => setSelectedIds(new Set(rows.map((row) => row.itemId)))}><ListFilter size={13} /> Select filtered</Button>
                  <Button size="sm" onClick={() => setSelectedIds(new Set())}>Clear</Button>
                </div>
                <Button data-testid="composition-confirm" variant="success" disabled={busy || !closedSelection(draft, selectedIds)} onClick={() => void confirmSelected()}><FileCheck2 size={14} /> Confirm {selectedIds.size}</Button>
              </div>
              {selectedIds.size > 0 && !closedSelection(draft, selectedIds) && <Notice variant="warn" title="Open selection">Include every pinned dependency before partial confirmation.</Notice>}
              <div className="mt-3 border border-rule-soft rounded-sm overflow-hidden">
                {paged.rows.map((row) => (
                  <div key={row.itemId} className="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-3 items-center p-2 border-b border-rule-soft last:border-b-0">
                    <input type="checkbox" aria-label={`Select ${row.itemId}`} checked={selectedIds.has(row.itemId)} onChange={(event) => setSelectedIds((values) => { const next = new Set(values); if (event.target.checked) next.add(row.itemId); else next.delete(row.itemId); return next; })} />
                    <button type="button" className="text-left min-w-0" onClick={() => setSelectedNodeId(row.itemId)}>
                      <span className="font-mono text-xs block truncate">{row.itemId}</span>
                      <span className="text-xs text-ink-mute">{row.node.definition.itemType} · {row.status}</span>
                    </button>
                    <Button size="sm" variant="danger" title="Remove node" disabled={row.itemId === draft.rootItemId || busy} onClick={() => void removeNode(row.itemId)}><Trash2 size={13} /></Button>
                  </div>
                ))}
              </div>
              <div className="flex justify-end items-center gap-2 mt-3">
                <Button size="sm" title="Previous page" disabled={paged.page === 1} onClick={() => setPage(paged.page - 1)}><ChevronLeft size={14} /></Button>
                <span className="font-mono text-xs">{paged.page} / {paged.pageCount}</span>
                <Button size="sm" title="Next page" disabled={paged.page === paged.pageCount} onClick={() => setPage(paged.page + 1)}><ChevronRight size={14} /></Button>
              </div>
              {selectedNode && (
                <CardSection title={`Edit ${selectedNode.definition.itemId}`}>
                  <Field label="Behavior intent" hint="One intent per line.">
                    <textarea data-testid="composition-node-behavior" className="min-h-28" defaultValue={selectedNode.definition.behaviorIntent.join("\n")} onBlur={(event) => void saveBehavior(event.target.value)} />
                  </Field>
                  <div className="flex gap-2 flex-wrap">
                    <Button size="sm" disabled={busy} onClick={() => void saveBehavior(selectedNode.definition.behaviorIntent.join("\n"))}><Save size={13} /> Save node</Button>
                    <Button size="sm" disabled={busy || !definitions.some((value) => value.definition.itemId === selectedNode.definition.itemId && value.definition.itemType === selectedNode.definition.itemType)} onClick={() => void replaceNode()}><RefreshCw size={13} /> Replace from library</Button>
                  </div>
                  <Field label="Retry instructions" hint="The model revises this node only; the complete Draft is revalidated before revision advances.">
                    <textarea
                      data-testid="composition-retry-instructions"
                      className="min-h-24"
                      value={retryInstructions}
                      onChange={(event) => setRetryInstructions(event.target.value)}
                    />
                  </Field>
                  <div className="flex items-center gap-2 flex-wrap">
                    <Button
                      data-testid="composition-retry-node"
                      size="sm"
                      disabled={busy || !retryInstructions.trim()}
                      onClick={() => void retryNode()}
                    >
                      <RefreshCw size={13} /> Retry node
                    </Button>
                    {lastRetryRun && <Badge variant={lastRetryRun.status === "succeeded" ? "ok" : lastRetryRun.status === "failed" ? "error" : "warn"}>{lastRetryRun.status}</Badge>}
                  </div>
                  {lastRetryRun?.failure && (
                    <Notice variant="error" title={lastRetryRun.failure.code}>
                      {lastRetryRun.failure.stage}
                    </Notice>
                  )}
                </CardSection>
              )}
            </>
          )}
        </Card>
      </div>
    </div>
  );
}

function definitionKey(definition: StoredItemDefinition): string {
  return `${definition.definition.itemId}:${definition.definitionHash}`;
}
