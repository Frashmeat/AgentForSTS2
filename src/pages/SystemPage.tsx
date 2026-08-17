import { useEffect, useState } from "react";
import { ExternalLink, RefreshCw, Save } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, Field, PageHero } from "@/components/ui";
import { buildPatch, formFromSnapshot, validateMaxOutputTokens, type FormState } from "@/pages/systemForm";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import type { SettingsSnapshot } from "@/services/tauriApi";

export function SystemPage() {
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [form, setForm] = useState<FormState | null>(null);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);
  const [busy, setBusy] = useState(false);
  const maxOutputTokensError = form ? validateMaxOutputTokens(form.llmMaxOutputTokens) : null;

  async function reload() {
    setBusy(true); setFailure(null);
    try {
      const value = await api.getSettingsSnapshot() as SettingsSnapshot;
      setSnapshot(value); setForm(formFromSnapshot(value));
    } catch (error: unknown) { setFailure(toActionableFailure(error)); }
    finally { setBusy(false); }
  }
  useEffect(() => { if (__IS_TAURI__) void reload(); }, []);

  async function save() {
    if (!snapshot || !form) return;
    setBusy(true); setFailure(null);
    try {
      const value = await api.saveSettingsPatch(buildPatch(form, snapshot)) as SettingsSnapshot;
      setSnapshot(value); setForm(formFromSnapshot(value));
    } catch (error: unknown) { setFailure(toActionableFailure(error)); }
    finally { setBusy(false); }
  }

  if (!__IS_TAURI__) return <Card title="Web configuration">Deployment owns Web settings.</Card>;
  return (
    <div className="space-y-4">
      <PageHero eyebrow="runtime · configuration" title="System" subtitle={snapshot?.configPath ?? "Loading"} actions={
        <div className="flex gap-2">
          <Button size="sm" onClick={() => void reload()} disabled={busy}><RefreshCw size={15} /> Reload</Button>
          <Button size="sm" variant="accent" onClick={() => void api.openConfigInEditor()} disabled={busy}><ExternalLink size={15} /> Open</Button>
          <Button data-testid="settings-save" size="sm" variant="success" onClick={() => void save()} disabled={busy || !form || Boolean(maxOutputTokensError)}><Save size={15} /> Save</Button>
        </div>
      } />
      {failure && <div data-testid="settings-error"><ActionableErrorNotice failure={failure} /></div>}
      {snapshot && form && (
        <>
          <Card eyebrow="model" title="LLM" actions={<Badge variant={snapshot.llm.apiKeyConfigured ? "ok" : "warn"}>{snapshot.llm.apiKeyConfigured ? "configured" : "missing key"}</Badge>}>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <Field label="Provider"><input className="input-mono" value={form.llmProvider} onChange={(event) => setForm({ ...form, llmProvider: event.target.value })} /></Field>
              <Field label="Model"><input className="input-mono" value={form.llmModel} onChange={(event) => setForm({ ...form, llmModel: event.target.value })} /></Field>
            </div>
            <Field label="Base URL"><input className="input-mono" value={form.llmBaseUrl} onChange={(event) => setForm({ ...form, llmBaseUrl: event.target.value })} /></Field>
            <Field label="Custom instructions"><textarea className="input-mono min-h-32" value={form.llmCustomPrompt} onChange={(event) => setForm({ ...form, llmCustomPrompt: event.target.value })} /></Field>
            <Field label="Maximum output tokens" hint={maxOutputTokensError}>
              <input data-testid="llm-max-output-tokens" type="number" min={1} max={65536} step={1} className="input-mono" value={form.llmMaxOutputTokens} aria-invalid={Boolean(maxOutputTokensError)} onChange={(event) => setForm({ ...form, llmMaxOutputTokens: event.target.value })} />
            </Field>
            <Field label="API key"><input type="password" className="input-mono" value={form.llmApiKey} placeholder={snapshot.llm.apiKeyConfigured ? "unchanged" : ""} onChange={(event) => setForm({ ...form, llmApiKey: event.target.value, llmApiKeyTouched: true })} /></Field>
          </Card>
          <Card eyebrow="media" title="Image generation" actions={<Badge variant={snapshot.imageGen.apiKeyConfigured ? "ok" : "warn"}>{snapshot.imageGen.apiKeyConfigured ? "configured" : "missing key"}</Badge>}>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <Field label="Provider"><input className="input-mono" value={form.imageProvider} onChange={(event) => setForm({ ...form, imageProvider: event.target.value })} /></Field>
              <Field label="Model"><input className="input-mono" value={form.imageModel} onChange={(event) => setForm({ ...form, imageModel: event.target.value })} /></Field>
              <Field label="Base URL"><input className="input-mono" value={form.imageBaseUrl} onChange={(event) => setForm({ ...form, imageBaseUrl: event.target.value })} /></Field>
              <Field label="Size"><input className="input-mono" value={form.imageSize} onChange={(event) => setForm({ ...form, imageSize: event.target.value })} /></Field>
            </div>
            <Field label="API key"><input type="password" className="input-mono" value={form.imageApiKey} placeholder={snapshot.imageGen.apiKeyConfigured ? "unchanged" : ""} onChange={(event) => setForm({ ...form, imageApiKey: event.target.value, imageApiKeyTouched: true })} /></Field>
          </Card>
          <Card eyebrow="local" title="Paths and credentials">
            <Field label="STS2 assembly"><input data-testid="sts2-dll-path" className="input-mono" value={form.sts2DllPath} onChange={(event) => setForm({ ...form, sts2DllPath: event.target.value })} /></Field>
            <Field label="Godot executable"><input data-testid="godot-exe-path" className="input-mono" value={form.godotExePath} onChange={(event) => setForm({ ...form, godotExePath: event.target.value })} /></Field>
            <Field label="GitHub token"><input type="password" className="input-mono" value={form.githubToken} placeholder={snapshot.runtimeWorkstation.githubTokenMasked !== "<empty>" ? "unchanged" : ""} onChange={(event) => setForm({ ...form, githubToken: event.target.value, githubTokenTouched: true })} /></Field>
          </Card>
        </>
      )}
    </div>
  );
}
