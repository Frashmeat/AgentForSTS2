import { useEffect, useMemo, useState } from "react";
import { BookOpen, Play, Plus, Save, WandSparkles } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, CardSection, Field, Notice, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import { waitForRun } from "@/services/runPolling";
import type {
  CurrentProject,
  ItemCapabilityCatalog,
  ItemDefinition,
  ItemFieldSpec,
  ItemFieldValue,
  PlanItem,
  RunRecord,
  StoredItemDefinition,
} from "@/services/tauriApi";
import {
  capabilityReason,
  confirmLocalization,
  createItemDraft,
  draftIssues,
  localizedLabel,
  setBehaviorText,
  setFieldValue,
  setLocalizationText,
} from "./itemEditorModel";
import { ResourceWorkbench } from "./ResourceWorkbench";

export function ModEditorPage() {
  const [project, setProject] = useState<CurrentProject | null>(null);
  const [catalog, setCatalog] = useState<ItemCapabilityCatalog | null>(null);
  const [items, setItems] = useState<StoredItemDefinition[]>([]);
  const [draft, setDraft] = useState<ItemDefinition | null>(null);
  const [storedDefinition, setStoredDefinition] = useState<StoredItemDefinition | null>(null);
  const [newItemId, setNewItemId] = useState("");
  const [newItemType, setNewItemType] = useState("");
  const [primaryLocale, setPrimaryLocale] = useState("eng");
  const [failure, setFailure] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);

  const [requirements, setRequirements] = useState("");
  const [generationItemType, setGenerationItemType] = useState("");
  const [artifactId, setArtifactId] = useState("mod-item");
  const [plan, setPlan] = useState<PlanItem | null>(null);
  const [run, setRun] = useState<RunRecord | null>(null);
  const [resourceIssues, setResourceIssues] = useState<string[]>([]);

  const selectedCapability = useMemo(
    () => catalog?.itemTypes.find((item) => item.descriptor.id === draft?.itemType) ?? null,
    [catalog, draft?.itemType],
  );
  const issues = useMemo(
    () => draft && selectedCapability ? draftIssues(draft, selectedCapability.descriptor) : [],
    [draft, selectedCapability],
  );
  const currentStoredDefinition = useMemo(
    () => draft && storedDefinition && JSON.stringify(draft) === JSON.stringify(storedDefinition.definition)
      ? storedDefinition
      : null,
    [draft, storedDefinition],
  );

  useEffect(() => {
    if (!__IS_TAURI__) return;
    void loadWorkspace();
  }, []);

  async function loadWorkspace() {
    setFailure(null);
    try {
      const current = await api.currentProject();
      setProject(current);
      const capabilities = await api.getItemCapabilities();
      setCatalog(capabilities);
      const firstReady = capabilities.itemTypes.find((item) => item.ready)?.descriptor.id ?? "";
      setNewItemType((value) => value || firstReady);
      setGenerationItemType((value) => value || firstReady);
      setItems(current ? await api.listItemDefinitions() : []);
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    }
  }

  function createDraft() {
    const capability = catalog?.itemTypes.find((item) => item.descriptor.id === newItemType);
    if (!capability?.ready) return;
    const next = createItemDraft(capability, newItemId.trim());
    setDraft(next);
    setStoredDefinition(null);
    setPrimaryLocale(capability.descriptor.requiredLocales[0] ?? "eng");
    setGenerationItemType(next.itemType);
    setArtifactId(next.itemId || "mod-item");
    setPlan(null);
    setResourceIssues([]);
  }

  function openItem(item: StoredItemDefinition) {
    setDraft(item.definition);
    setStoredDefinition(item);
    const capability = catalog?.itemTypes.find(
      (candidate) => candidate.descriptor.id === item.definition.itemType,
    );
    setPrimaryLocale(capability?.descriptor.requiredLocales[0] ?? "eng");
    setGenerationItemType(item.definition.itemType);
    setArtifactId(item.definition.itemId);
    setRequirements(item.definition.behaviorIntent.join("\n"));
    setPlan(null);
    setResourceIssues([]);
  }

  async function saveDraft() {
    if (!draft || issues.length > 0 || !selectedCapability?.ready) return;
    setBusy(true);
    setFailure(null);
    try {
      const stored = await api.saveItemDefinition(draft);
      setDraft(stored.definition);
      setStoredDefinition(stored);
      setItems(await api.listItemDefinitions());
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function planMod() {
    setBusy(true);
    setFailure(null);
    setPlan(null);
    try {
      const id = await api.submitModPlan({ requirements, itemType: generationItemType });
      const terminal = await waitForRun(id, setRun);
      if (terminal.status === "succeeded") {
        const value = decodePlan(terminal);
        if (value && draft) setPlan({ ...value, itemId: draft.itemId, itemType: draft.itemType });
      }
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  async function generate() {
    if (!plan || !project || !currentStoredDefinition) return;
    setBusy(true);
    setFailure(null);
    try {
      const id = await api.submitSingleGenerate({
        artifactId,
        modId: project.csharpName,
        plan,
        definition: currentStoredDefinition,
      });
      await waitForRun(id, setRun);
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-4">
      <PageHero
        eyebrow="item library · pack-driven editor"
        title="Mod items"
        subtitle={project?.name ?? "No project open"}
        actions={<Button onClick={() => void loadWorkspace()}><BookOpen size={14} /> Refresh</Button>}
      />
      {failure && <div data-testid="item-editor-error"><ActionableErrorNotice failure={failure} /></div>}
      {!project && <Notice variant="warn" title="Project required">Open a project from Dashboard.</Notice>}
      {catalog && (
        <Notice variant={catalog.truthSnapshotId ? "ok" : "warn"} title="Pack capability identity">
          {catalog.gamePackId} · {catalog.gamePackSha256.slice(0, 12)} · Truth {catalog.truthSnapshotId?.slice(0, 12) ?? "unavailable"}
        </Notice>
      )}

      <div className="grid grid-cols-1 xl:grid-cols-[minmax(260px,0.7fr)_minmax(0,1.8fr)] gap-4">
        <Card eyebrow="library" title="Saved items" subtitle="Current pointers; older definition hashes remain immutable.">
          <Field label="New item type">
            <select data-testid="new-item-type" value={newItemType} onChange={(event) => setNewItemType(event.target.value)}>
              {catalog?.itemTypes.map((capability) => (
                <option key={capability.descriptor.id} value={capability.descriptor.id} disabled={!capability.ready}>
                  {localizedLabel(capability.descriptor.displayNames)} · {capabilityReason(capability)}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Stable item ID" hint="Lowercase letters, digits, underscore or hyphen.">
            <input data-testid="new-item-id" className="input-mono" value={newItemId} onChange={(event) => setNewItemId(event.target.value)} placeholder="burning-blood" />
          </Field>
          <Button data-testid="new-item-submit" variant="accent" disabled={!project || !newItemId.trim() || busy} onClick={createDraft}>
            <Plus size={14} /> New item
          </Button>
          <CardSection title="Current definitions">
            <div className="space-y-2">
              {items.length === 0 && <p className="text-sm text-ink-mute">No saved items.</p>}
              {items.map((item) => (
                <button
                  type="button"
                  key={item.definition.itemId}
                  className="w-full text-left border border-rule-soft rounded-sm p-2 hover:border-accent"
                  onClick={() => openItem(item)}
                >
                  <span className="block font-mono text-xs">{item.definition.itemId}</span>
                  <span className="block text-xs text-ink-mute">
                    {item.definition.itemType} · {item.definitionHash.slice(0, 10)}
                  </span>
                </button>
              ))}
            </div>
          </CardSection>
          <CardSection title="Declared capabilities">
            <div className="flex flex-wrap gap-2">
              {catalog?.itemTypes.map((capability) => (
                <Badge key={capability.descriptor.id} variant={capability.ready ? "ok" : "warn"}>
                  {capability.descriptor.id}: {capabilityReason(capability)}
                </Badge>
              ))}
            </div>
          </CardSection>
        </Card>

        <Card
          eyebrow="definition v1"
          title={draft?.itemId || "Create or open an item"}
          subtitle={draft ? `${draft.itemType} · ${currentStoredDefinition?.definitionHash ?? "unsaved changes"}` : "The form is rendered from the selected Pack descriptor."}
          actions={draft && <Button data-testid="item-save" variant="success" disabled={busy || issues.length > 0 || !selectedCapability?.ready} onClick={() => void saveDraft()}><Save size={14} /> Save snapshot</Button>}
        >
          {!draft || !selectedCapability ? (
            <Notice variant="muted" title="No item selected">Choose a ready Pack type or open an existing definition.</Notice>
          ) : (
            <>
              {issues.length > 0 && <Notice variant="warn" title="Draft checks">{issues.join(" ")}</Notice>}
              <CardSection title="Canonical fields">
                {selectedCapability.descriptor.fields.length === 0 && <p className="text-sm text-ink-mute">This type declares no structured fields.</p>}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {selectedCapability.descriptor.fields.map((field) => (
                    <CanonicalFieldEditor key={field.id} field={field} definition={draft} onChange={setDraft} />
                  ))}
                </div>
              </CardSection>
              <CardSection title="Behavior intent">
                <Field label="One intent per line" hint="Structured fields remain canonical; use this area for behavior that a form cannot express.">
                  <textarea data-testid="item-behavior-intent" className="input-mono min-h-32" value={draft.behaviorIntent.join("\n")} onChange={(event) => setDraft(setBehaviorText(draft, event.target.value))} />
                </Field>
              </CardSection>
              <CardSection title="Localization">
                {selectedCapability.descriptor.requiredLocales.length === 0 ? (
                  <p className="text-sm text-ink-mute">This type does not require localized output.</p>
                ) : (
                  <>
                    <Field label="Primary editing locale">
                      <select value={primaryLocale} onChange={(event) => setPrimaryLocale(event.target.value)}>
                        {selectedCapability.descriptor.requiredLocales.map((locale) => <option key={locale}>{locale}</option>)}
                      </select>
                    </Field>
                    <Notice variant="muted" title="Candidate-only translation">
                      Secondary locale edits remain outdated until explicitly confirmed. Future AI translation writes the same candidate state and cannot silently replace confirmed text.
                    </Notice>
                    <div className="grid grid-cols-1 lg:grid-cols-2 gap-3 mt-3">
                      {selectedCapability.descriptor.requiredLocales.map((locale) => {
                        const value = draft.localizations[locale];
                        const status = value?.status ?? "draft";
                        return (
                          <div key={locale} className="border border-rule-soft rounded-sm p-3 space-y-2">
                            <div className="flex justify-between items-center">
                              <strong className="font-mono text-xs">{locale}</strong>
                              <Badge variant={status === "confirmed" ? "ok" : "warn"}>{status}</Badge>
                            </div>
                            <Field label="Name"><input value={value?.name ?? ""} onChange={(event) => setDraft(setLocalizationText(draft, locale, primaryLocale, { name: event.target.value }))} /></Field>
                            <Field label="Description"><textarea className="min-h-24" value={value?.description ?? ""} onChange={(event) => setDraft(setLocalizationText(draft, locale, primaryLocale, { description: event.target.value }))} /></Field>
                            {locale !== primaryLocale && <Button size="sm" disabled={!value?.name.trim() || !value.description.trim()} onClick={() => setDraft(confirmLocalization(draft, locale))}>Confirm candidate</Button>}
                          </div>
                        );
                      })}
                    </div>
                  </>
                )}
              </CardSection>
              <CardSection title="Resources">
                <ResourceWorkbench
                  definition={draft}
                  requiredRoles={selectedCapability.descriptor.requiredResourceRoles}
                  onChange={setDraft}
                  onRun={setRun}
                  onFailure={setFailure}
                  onIssuesChange={setResourceIssues}
                />
                {resourceIssues.length > 0 && <Notice variant="warn" title="Resource checks">{resourceIssues.join(" ")}</Notice>}
              </CardSection>
              <CardSection title="Typed definition preview">
                {currentStoredDefinition && <div data-testid="item-saved-hash" className="font-mono text-xs mb-2">{currentStoredDefinition.definitionHash}</div>}
                <pre className="pre-block max-h-72">{JSON.stringify(draft, null, 2)}</pre>
              </CardSection>
            </>
          )}
        </Card>
      </div>

      <Card eyebrow="definition-bound generation" title="Plan and generate" subtitle={currentStoredDefinition ? `Pinned definition ${currentStoredDefinition.definitionHash.slice(0, 12)}` : "Save the current definition snapshot before generation."}>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 mt-3">
          <div className="space-y-3">
            <Field label="Ready item type">
              <select value={generationItemType} onChange={(event) => setGenerationItemType(event.target.value)}>
                {catalog?.itemTypes.map((capability) => <option key={capability.descriptor.id} value={capability.descriptor.id} disabled={!capability.ready}>{localizedLabel(capability.descriptor.displayNames)} · {capabilityReason(capability)}</option>)}
              </select>
            </Field>
            <Field label="Requirements"><textarea className="input-mono min-h-36" value={requirements} onChange={(event) => setRequirements(event.target.value)} /></Field>
            <Button variant="accent" disabled={busy || !project || !requirements.trim() || !generationItemType} onClick={() => void planMod()}><WandSparkles size={15} /> Plan</Button>
          </div>
          <div className="space-y-3">
            {plan && <pre className="pre-block max-h-48">{JSON.stringify(plan, null, 2)}</pre>}
            <Field label="Artifact ID"><input className="input-mono" value={artifactId} onChange={(event) => setArtifactId(event.target.value)} /></Field>
            <Button variant="success" disabled={busy || !plan || !artifactId.trim() || !currentStoredDefinition || resourceIssues.length > 0} onClick={() => void generate()}><Play size={15} /> Generate</Button>
          </div>
        </div>
      </Card>
      {run && <RunResult run={run} />}
    </div>
  );
}

function CanonicalFieldEditor({
  field,
  definition,
  onChange,
}: {
  field: ItemFieldSpec;
  definition: ItemDefinition;
  onChange: (definition: ItemDefinition) => void;
}) {
  const current = definition.canonicalFields[field.id];
  const label = `${localizedLabel(field.displayNames)}${field.required ? " *" : ""}`;
  const update = (value?: ItemFieldValue) => onChange(setFieldValue(definition, field.id, value));
  switch (field.value.kind) {
    case "text":
      return <Field label={label}>{field.value.multiline ? <textarea value={current?.kind === "text" ? current.value : ""} onChange={(event) => update(event.target.value ? { kind: "text", value: event.target.value } : undefined)} /> : <input value={current?.kind === "text" ? current.value : ""} onChange={(event) => update(event.target.value ? { kind: "text", value: event.target.value } : undefined)} />}</Field>;
    case "integer":
      return <Field label={label} hint={`${field.value.min}–${field.value.max}`}><input type="number" min={field.value.min} max={field.value.max} value={current?.kind === "integer" ? current.value : ""} onChange={(event) => update(event.target.value === "" ? undefined : { kind: "integer", value: Number(event.target.value) })} /></Field>;
    case "boolean":
      return <Field label={label}><input type="checkbox" checked={current?.kind === "boolean" && current.value} onChange={(event) => update({ kind: "boolean", value: event.target.checked })} /></Field>;
    case "choice":
      return <Field label={label}><select value={current?.kind === "choice" ? current.value : ""} onChange={(event) => update(event.target.value ? { kind: "choice", value: event.target.value } : undefined)}><option value="">Select…</option>{field.value.options.map((option) => <option key={option.value} value={option.value}>{localizedLabel(option.displayNames)}</option>)}</select></Field>;
    case "string_list":
      return <Field label={label} hint="One value per line"><textarea value={current?.kind === "string_list" ? current.value.join("\n") : ""} onChange={(event) => { const values = event.target.value.split("\n").map((value) => value.trim()).filter(Boolean); update(values.length > 0 ? { kind: "string_list", value: values } : undefined); }} /></Field>;
  }
}

function decodePlan(run: RunRecord): PlanItem | null {
  if (run.result?.schema.id !== "feature.mod-plan-result" || run.result.schema.version !== 2) return null;
  const value = run.result.payload;
  return isPlanItem(value) ? value : null;
}

function isPlanItem(value: Record<string, unknown>): value is PlanItem {
  return typeof value.itemId === "string" && typeof value.itemType === "string" && typeof value.name === "string" && typeof value.summary === "string" && isStringArray(value.behaviorIntent) && isStringArray(value.implementationConstraints) && isStringArray(value.evidenceRequirements) && isStringArray(value.requiredResourceRoles) && isStringArray(value.acceptanceCriteria);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function RunResult({ run }: { run: RunRecord }) {
  return <Card eyebrow="run" title={run.featureId} actions={<Badge variant={run.status === "succeeded" ? "ok" : run.status === "failed" ? "error" : "warn"}>{run.status}</Badge>}>{run.failure && <Notice variant="error" title={run.failure.code}>{run.failure.stage}</Notice>}{run.result && <pre className="pre-block max-h-80">{JSON.stringify(run.result.payload, null, 2)}</pre>}</Card>;
}
