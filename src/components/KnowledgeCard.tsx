import { useEffect, useRef, useState } from "react";
import { Pencil, RefreshCw, Save, Search, X } from "lucide-react";

import { Badge, Button, Card, CardSection, Notice } from "@/components/ui";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { useRunProgress } from "@/hooks/useRunProgress";
import { api } from "@/services/api";
import {
  localValidationFailure,
  toActionableFailure,
} from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
import type {
  SettingsSnapshot,
  SubmitRunAck,
  TruthSnapshotStatus,
} from "@/services/tauriApi";
import { useProjectStore } from "@/stores/project";

export function KnowledgeCard() {
  const [snapshot, setSnapshot] = useState<TruthSnapshotStatus | null>(null);
  const [error, setError] = useState<ActionableFailure | null>(null);
  const [checking, setChecking] = useState(false);
  const [sts2Path, setSts2Path] = useState("");
  const [editingPath, setEditingPath] = useState(false);
  const [force, setForce] = useState(false);
  const [refreshBusy, setRefreshBusy] = useState(false);
  const [refreshStage, setRefreshStage] = useState<string | null>(null);
  const [refreshMessage, setRefreshMessage] = useState<string | null>(null);
  const [refreshRunId, setRefreshRunId] = useState<string | null>(null);
  const refreshRunIdRef = useRef<string | null>(null);
  const project = useProjectStore((state) => state.project);

  useEffect(() => {
    refreshRunIdRef.current = refreshRunId;
  }, [refreshRunId]);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    setSnapshot(null);
    setError(null);
    if (project) void loadStatus();
  }, [project?.path]);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    (api.getSettingsSnapshot() as Promise<SettingsSnapshot>)
      .then((settings) => setSts2Path(settings.knowledge.sts2DllPath))
      .catch(() => {});
  }, []);

  useRunProgress(refreshRunIdRef, (event) => {
    setRefreshStage(event.stage);
    if (event.message) setRefreshMessage(event.message);
    if (
      event.stage === "completed" ||
      event.stage === "failed" ||
      event.stage.includes("error")
    ) {
      setRefreshBusy(false);
      void loadStatus();
    }
  });

  async function loadStatus(check = false) {
    setError(null);
    try {
      const result = check
        ? await api.checkTruthSnapshotStatus()
        : await api.getTruthSnapshotStatus();
      setSnapshot(result as TruthSnapshotStatus);
    } catch (caught: unknown) {
      setError(toActionableFailure(caught));
    }
  }

  async function handleRefresh() {
    setError(null);
    setRefreshBusy(true);
    setRefreshStage(null);
    setRefreshMessage(null);
    try {
      const ack = (await api.submitTruthSnapshotRefreshRun({ force })) as SubmitRunAck;
      setRefreshRunId(ack.runId);
    } catch (caught: unknown) {
      setError(toActionableFailure(caught));
      setRefreshBusy(false);
    }
  }

  async function handleRecheck() {
    setChecking(true);
    await loadStatus(true);
    setChecking(false);
  }

  if (!__IS_TAURI__) {
    return (
      <Card
        eyebrow="game pack · truth snapshot"
        title="Truth Snapshot"
        subtitle="desktop-only"
      />
    );
  }

  const stateVariant =
    snapshot?.state === "ready"
      ? "ok"
      : snapshot?.state === "invalid"
        ? "error"
        : "warn";

  return (
    <Card
      eyebrow="game pack · verified sources"
      title="Truth Snapshot"
      actions={
        <Button
          size="sm"
          onClick={() => void handleRecheck()}
          disabled={checking}
        >
          <RefreshCw size={14} />
          {checking ? "Checking..." : "Re-check"}
        </Button>
      }
    >
      <ActionableErrorNotice failure={error} />
      {!project && <Notice variant="warn" title="Open a project to select its Game Pack." />}
      {!error && project && !snapshot && (
        <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Loading...</p>
      )}

      {snapshot && (
        <>
          <div className="flex items-center gap-3 flex-wrap mb-4">
            <Badge variant={stateVariant}>{snapshot.state}</Badge>
            <span style={{ fontSize: "12px", color: "var(--ink-mute)" }}>
              pack <code>{snapshot.gamePackId}</code>
            </span>
            {snapshot.snapshotId && (
              <span style={{ fontSize: "12px", color: "var(--ink-mute)" }}>
                snapshot <code>{snapshot.snapshotId.slice(0, 12)}</code>
              </span>
            )}
          </div>

          {snapshot.warnings.map((warning) => (
            <Notice key={warning} variant="warn" title={warning} />
          ))}

          {snapshot.sources.length > 0 && (
            <CardSection title="Verified sources">
              <div className="space-y-2">
                {snapshot.sources.map((source) => {
                  const index = snapshot.indexes.find(
                    (candidate) => candidate.sourceId === source.id,
                  );
                  return (
                    <div
                      key={source.id}
                      className="grid gap-1 py-2"
                      style={{ borderBottom: "1px solid var(--rule-soft)" }}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <strong style={{ fontSize: "13px" }}>{source.id}</strong>
                        <code style={{ fontSize: "11px" }}>
                          {source.sha256.slice(0, 12)}
                        </code>
                      </div>
                      <div style={{ color: "var(--ink-mute)", fontSize: "12px" }}>
                        {source.kind}
                        {source.version ? ` · ${source.version}` : ""}
                        {index
                          ? ` · ${index.indexer} · ${index.csFileCount} C# files`
                          : ""}
                      </div>
                    </div>
                  );
                })}
              </div>
              {Object.keys(snapshot.toolVersions).length > 0 && (
                <p className="mt-3" style={{ color: "var(--ink-mute)", fontSize: "12px" }}>
                  {Object.entries(snapshot.toolVersions)
                    .map(([tool, version]) => `${tool} ${version}`)
                    .join(" · ")}
                </p>
              )}
            </CardSection>
          )}

          <CardSection title="Local game assembly">
            {editingPath ? (
              <>
                <div className="flex gap-2">
                  <input
                    value={sts2Path}
                    onChange={(event) => setSts2Path(event.target.value)}
                    placeholder="sts2.dll full path"
                    className="input-mono flex-1"
                  />
                  <Button
                    size="sm"
                    title="Discover sts2.dll"
                    aria-label="Discover sts2.dll"
                    onClick={async () => {
                      try {
                        const found = (await api.discoverSts2Dll()) as string | null;
                        if (found) setSts2Path(found);
                        else setError(localValidationFailure(
                          "knowledge.discover",
                          "sts2.dll was not found.",
                        ));
                      } catch (caught) {
                        setError(toActionableFailure(caught));
                      }
                    }}
                  >
                    <Search size={14} />
                  </Button>
                </div>
                <div className="flex gap-2 mt-2">
                  <Button
                    variant="success"
                    size="sm"
                    onClick={async () => {
                      try {
                        await api.saveSettingsPatch({
                          knowledge: { sts2_dll_path: sts2Path },
                        });
                        setEditingPath(false);
                      } catch (caught) {
                        setError(toActionableFailure(caught));
                      }
                    }}
                  >
                    <Save size={14} /> Save
                  </Button>
                  <Button size="sm" onClick={() => setEditingPath(false)}>
                    <X size={14} /> Cancel
                  </Button>
                </div>
              </>
            ) : (
              <div className="flex items-center gap-2">
                <code className="break-all flex-1" style={{ fontSize: "12px" }}>
                  {sts2Path || "<not set>"}
                </code>
                <Button
                  size="sm"
                  title="Edit local game assembly"
                  aria-label="Edit local game assembly"
                  onClick={() => setEditingPath(true)}
                >
                  <Pencil size={14} />
                </Button>
              </div>
            )}
          </CardSection>

          <CardSection title="Refresh">
            <div className="flex items-center gap-3 flex-wrap">
              <label className="flex items-center gap-2" style={{ fontSize: "13px" }}>
                <input
                  type="checkbox"
                  checked={force}
                  onChange={(event) => setForce(event.target.checked)}
                />
                force re-index
              </label>
              <Button
                variant="accent"
                onClick={() => void handleRefresh()}
                disabled={refreshBusy || !sts2Path || !project}
              >
                <RefreshCw size={14} />
                {refreshBusy ? (refreshStage ?? "Refreshing...") : "Refresh snapshot"}
              </Button>
              {refreshMessage && (
                <span style={{ fontSize: "12px", color: "var(--ink-mute)" }}>
                  {refreshMessage}
                </span>
              )}
            </div>
          </CardSection>
        </>
      )}
    </Card>
  );
}
