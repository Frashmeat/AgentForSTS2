import { useEffect, useMemo, useState } from "react";
import { Check, Eye, ImagePlus, PackageOpen, RefreshCw, Sparkles } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";

import { Badge, Button, Field } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import { waitForRun } from "@/services/runPolling";
import type {
  ItemDefinition,
  ResourceAsset,
  ResourceCatalog,
  ResourcePreview,
  RunRecord,
} from "@/services/tauriApi";
import {
  bindSelectedResource,
  candidatesForRole,
  resourceBindingIssues,
  workbenchRoles,
} from "./resourceWorkbenchModel";

interface ResourceWorkbenchProps {
  definition: ItemDefinition;
  requiredRoles: string[];
  onChange: (definition: ItemDefinition) => void | Promise<void>;
  onRun: (run: RunRecord) => void;
  onFailure: (failure: ActionableFailure) => void;
  onIssuesChange: (issues: string[]) => void;
}

export function ResourceWorkbench({
  definition,
  requiredRoles,
  onChange,
  onRun,
  onFailure,
  onIssuesChange,
}: ResourceWorkbenchProps) {
  const [catalog, setCatalog] = useState<ResourceCatalog | null>(null);
  const [assets, setAssets] = useState<ResourceAsset[]>([]);
  const [previews, setPreviews] = useState<Record<string, ResourcePreview>>({});
  const [aiPrompt, setAiPrompt] = useState("");
  const [busyRole, setBusyRole] = useState<string | null>(null);
  const roles = useMemo(
    () => catalog ? workbenchRoles(catalog, requiredRoles) : [],
    [catalog, requiredRoles],
  );
  const issues = useMemo(
    () => catalog ? resourceBindingIssues(definition, requiredRoles, catalog, assets) : [],
    [assets, catalog, definition, requiredRoles],
  );

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    onIssuesChange(issues);
  }, [issues, onIssuesChange]);

  async function refresh() {
    try {
      const [nextCatalog, nextAssets] = await Promise.all([
        api.getResourceCatalog(),
        api.listResourceAssets(),
      ]);
      setCatalog(nextCatalog);
      setAssets(nextAssets);
    } catch (error: unknown) {
      onFailure(toActionableFailure(error));
    }
  }

  async function prepareUpload(roleId: string, mediaType: string) {
    const sourcePath = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "PNG", extensions: ["png"] }],
    });
    if (typeof sourcePath !== "string") return;
    await prepare(roleId, mediaType, { kind: "user_upload" }, sourcePath);
  }

  async function prepareAi(roleId: string, mediaType: string) {
    if (!aiPrompt.trim()) return;
    await prepare(roleId, mediaType, { kind: "ai_generated", prompt: aiPrompt.trim() });
  }

  async function prepareDefault(roleId: string, mediaType: string) {
    await prepare(roleId, mediaType, { kind: "pack_default" });
  }

  async function prepare(
    roleId: string,
    mediaType: string,
    source:
      | { kind: "user_upload" }
      | { kind: "pack_default" }
      | { kind: "ai_generated"; prompt: string },
    sourcePath?: string,
  ) {
    setBusyRole(roleId);
    try {
      const runId = await api.submitResourcePrepare(
        { logicalRole: roleId, mediaType, source },
        sourcePath,
      );
      const terminal = await waitForRun(runId, onRun);
      onRun(terminal);
      await refresh();
    } catch (error: unknown) {
      onFailure(toActionableFailure(error));
    } finally {
      setBusyRole(null);
    }
  }

  async function loadPreview(asset: ResourceAsset, version: string) {
    const key = `${asset.resourceId}:${version}`;
    if (previews[key]) return;
    try {
      const preview = await api.getResourcePreview(asset.resourceId, version);
      setPreviews((current) => ({ ...current, [key]: preview }));
    } catch (error: unknown) {
      onFailure(toActionableFailure(error));
    }
  }

  async function select(asset: ResourceAsset, version: string) {
    setBusyRole(asset.logicalRole);
    try {
      await api.selectResource(asset.resourceId, version);
      const nextAssets = await api.listResourceAssets();
      setAssets(nextAssets);
      const selected = nextAssets.find((candidate) => candidate.resourceId === asset.resourceId);
      if (!selected) throw new Error("selected resource disappeared");
      await onChange(bindSelectedResource(definition, asset.logicalRole, selected, version));
    } catch (error: unknown) {
      onFailure(toActionableFailure(error));
    } finally {
      setBusyRole(null);
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-col md:flex-row gap-2 md:items-end">
        <Field label="AI image intent">
          <input data-testid="resource-ai-prompt" value={aiPrompt} onChange={(event) => setAiPrompt(event.target.value)} />
        </Field>
        <Button size="sm" onClick={() => void refresh()} title="Refresh resources">
          <RefreshCw size={13} /> Refresh
        </Button>
      </div>
      {roles.map((role) => {
        const roleAssets = candidatesForRole(assets, role.id);
        const required = requiredRoles.includes(role.id);
        const binding = definition.resourceBindings[role.id];
        return (
          <section key={role.id} className="border-t border-rule-soft pt-3 first:border-t-0 first:pt-0">
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <strong className="font-mono text-xs">{role.id}</strong>
                  <Badge variant={required ? (binding ? "ok" : "warn") : "muted"}>
                    {required ? (binding ? "bound" : "required") : "master"}
                  </Badge>
                </div>
                <p className="text-xs text-ink-mute mt-1">
                  {role.width} x {role.height} · {role.mediaTypes.join(", ")} · alpha {role.requireAlpha ? "required" : "optional"}
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button size="sm" disabled={busyRole !== null} onClick={() => void prepareUpload(role.id, role.mediaTypes[0])}>
                  <ImagePlus size={13} /> Upload
                </Button>
                {role.packDefaultAvailable && (
                  <Button
                    size="sm"
                    disabled={busyRole !== null}
                    data-testid={`resource-default-${role.id}`}
                    onClick={() => void prepareDefault(role.id, role.mediaTypes[0])}
                    title="Prepare Pack default"
                  >
                    <PackageOpen size={13} /> Default
                  </Button>
                )}
                <Button data-testid={`resource-ai-${role.id}`} size="sm" variant="accent" disabled={busyRole !== null || !aiPrompt.trim()} onClick={() => void prepareAi(role.id, role.mediaTypes[0])}>
                  <Sparkles size={13} /> AI
                </Button>
              </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2 mt-3">
              {roleAssets.flatMap((asset) => asset.versions.map((version) => {
                const key = `${asset.resourceId}:${version.id}`;
                const preview = previews[key];
                const isSelected = asset.selectedVersion === version.id;
                const isBound = binding?.resourceId === asset.resourceId && binding.selectedVersion === version.id;
                return (
                  <div key={key} className="border border-rule-soft rounded-sm p-2 min-w-0">
                    <div className="aspect-square max-h-32 bg-paper border border-rule-hair grid place-items-center overflow-hidden">
                      {preview ? <img className="w-full h-full object-contain" src={preview.dataUrl} alt={role.id} /> : <Eye size={20} className="text-ink-faint" />}
                    </div>
                    <p className="font-mono text-[10px] text-ink-mute mt-2 break-all">{version.id.slice(0, 16)}</p>
                    <div className="flex flex-wrap items-center gap-1 mt-1">
                      <span className="text-xs text-ink-mute">{asset.origin.kind.replaceAll("_", " ")}</span>
                      {isSelected && <Badge variant="ok">selected</Badge>}
                      {isBound && <Badge variant="accent">bound</Badge>}
                    </div>
                    <div className="flex flex-wrap gap-1 mt-2">
                      <Button size="sm" onClick={() => void loadPreview(asset, version.id)} title="Preview candidate">
                        <Eye size={12} /> Preview
                      </Button>
                      {required && (
                        <Button size="sm" variant={isBound ? "success" : "accent"} disabled={busyRole !== null} onClick={() => void select(asset, version.id)}>
                          <Check size={12} /> {isBound ? "Bound" : "Select"}
                        </Button>
                      )}
                    </div>
                  </div>
                );
              }))}
            </div>
          </section>
        );
      })}
    </div>
  );
}
