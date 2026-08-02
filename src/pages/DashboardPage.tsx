import { useEffect, useState } from "react";
import { Database, FolderOpen, Plus, RefreshCw, X } from "lucide-react";

import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { Badge, Button, Card, Field, KV, KVList, PageHero } from "@/components/ui";
import { api } from "@/services/api";
import { toActionableFailure, type ActionableFailure } from "@/services/actionableFailure";
import type { CurrentProject, HealthReport, RecentEntry, TruthStatus } from "@/services/tauriApi";

export function DashboardPage() {
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [project, setProject] = useState<CurrentProject | null>(null);
  const [truth, setTruth] = useState<TruthStatus | null>(null);
  const [recents, setRecents] = useState<RecentEntry[]>([]);
  const [parentDir, setParentDir] = useState("");
  const [projectName, setProjectName] = useState("");
  const [openPath, setOpenPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<ActionableFailure | null>(null);

  async function reload() {
    setFailure(null);
    try {
      const nextHealth = await api.getHealth() as HealthReport;
      setHealth(nextHealth);
      if (__IS_TAURI__) {
        const [nextProject, nextTruth, nextRecents] = await Promise.all([
          api.currentProject(), api.getTruthStatus(), api.listRecentProjects(),
        ]);
        setProject(nextProject as CurrentProject | null);
        setTruth(nextTruth as TruthStatus);
        setRecents(nextRecents as RecentEntry[]);
      }
    } catch (error: unknown) {
      setFailure(toActionableFailure(error));
    }
  }

  useEffect(() => { void reload(); }, []);

  async function action(operation: () => Promise<unknown>) {
    setBusy(true); setFailure(null);
    try { await operation(); await reload(); }
    catch (error: unknown) { setFailure(toActionableFailure(error)); }
    finally { setBusy(false); }
  }

  return (
    <div className="space-y-4">
      <PageHero eyebrow="workspace · verified context" title="Dashboard" subtitle="Project, Pack and Truth readiness" actions={
        <Button size="sm" onClick={() => void reload()} disabled={busy} title="刷新">
          <RefreshCw size={15} /> Refresh
        </Button>
      } />
      <ActionableErrorNotice failure={failure} />
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card eyebrow="runtime" title="Build and contracts">
          <KVList>
            <KV k="status"><Badge variant={health?.status === "ok" ? "ok" : "warn"}>{health?.status ?? "loading"}</Badge></KV>
            <KV k="variant">{health?.build?.variant ?? "web"}</KV>
            <KV k="build_id">{health?.build?.buildId ?? "n/a"}</KV>
            <KV k="game_pack">{health?.gamePackId ?? "sts2"}</KV>
            <KV k="features">{health?.featureCount ?? 0}</KV>
            <KV k="truth">{truth?.ready ? "ready" : "missing"}</KV>
          </KVList>
          {__IS_TAURI__ && !truth?.ready && (
            <div className="mt-4">
              <Button variant="accent" disabled={busy} onClick={() => void action(() => api.importTruth())}>
                <Database size={15} /> Import Truth
              </Button>
            </div>
          )}
        </Card>
        <Card eyebrow="project" title={project?.name ?? "No project open"} actions={project && (
          <Button size="sm" variant="danger" disabled={busy} onClick={() => void action(() => api.closeProject())}>
            <X size={15} /> Close
          </Button>
        )}>
          {project ? (
            <KVList>
              <KV k="path"><code className="break-all">{project.path}</code></KV>
              <KV k="mod_id">{project.csharpName}</KV>
              <KV k="game_pack">{project.gameId}</KV>
              <KV k="state">{project.closing ? "closing" : "open"}</KV>
            </KVList>
          ) : __IS_TAURI__ ? (
            <div className="space-y-3">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Field label="Parent directory"><input className="input-mono" value={parentDir} onChange={(event) => setParentDir(event.target.value)} /></Field>
                <Field label="Project name"><input className="input-mono" value={projectName} onChange={(event) => setProjectName(event.target.value)} /></Field>
              </div>
              <Button variant="success" disabled={busy || !parentDir || !projectName} onClick={() => void action(() => api.createProject(parentDir, projectName))}>
                <Plus size={15} /> Create
              </Button>
              <Field label="Existing project"><input className="input-mono" value={openPath} onChange={(event) => setOpenPath(event.target.value)} /></Field>
              <Button variant="accent" disabled={busy || !openPath} onClick={() => void action(() => api.openProject(openPath))}>
                <FolderOpen size={15} /> Open
              </Button>
            </div>
          ) : <p>Desktop project execution is unavailable in the Web shell.</p>}
        </Card>
      </div>
      {__IS_TAURI__ && recents.length > 0 && !project && (
        <Card eyebrow="recent" title="Recent projects">
          <div className="space-y-2">
            {recents.map((entry) => (
              <button key={entry.path} type="button" className="w-full text-left topbtn ghost" onClick={() => void action(() => api.openProject(entry.path))}>
                <strong>{entry.name}</strong><br /><code className="break-all">{entry.path}</code>
              </button>
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}
