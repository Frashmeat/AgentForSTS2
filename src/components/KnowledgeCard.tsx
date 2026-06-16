import { useEffect, useRef, useState } from "react";
import { Badge, Button, Card, CardSection, Notice } from "@/components/ui";
import { api } from "@/services/api";
import { useJobProgress } from "@/hooks/useJobProgress";
import type {
  ExportPackStats,
  ImportPackStats,
  KnowledgeStatus,

  SubmitJobAck,
} from "@/services/tauriApi";

export function KnowledgeCard() {
  const [knowledge, setKnowledge] = useState<KnowledgeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [packPath, setPackPath] = useState("");
  const [packMsg, setPackMsg] = useState<string | null>(null);
  const [packBusy, setPackBusy] = useState(false);
  const [overwriteOnImport, setOverwriteOnImport] = useState(false);

  const [force, setForce] = useState(false);
  const [refreshBusy, setRefreshBusy] = useState(false);
  const [refreshStage, setRefreshStage] = useState<string | null>(null);
  const [refreshMsg, setRefreshMsg] = useState<string | null>(null);
  const [refreshJobId, setRefreshJobId] = useState<string | null>(null);
  const refreshJobIdRef = useRef<string | null>(null);

  useEffect(() => { refreshJobIdRef.current = refreshJobId; }, [refreshJobId]);

  useEffect(() => {
    (api.getKnowledgeStatus() as Promise<KnowledgeStatus>)
      .then(setKnowledge)
      .catch((e: unknown) => setError(String(e)));
  }, []);



  useJobProgress(refreshJobIdRef, (ev) => {
    setRefreshStage(ev.stage);
    if (ev.message) setRefreshMsg(ev.message);
    if (ev.stage === "completed" || ev.stage === "failed" || ev.stage.includes("error")) {
      setRefreshBusy(false);
      void (async () => {
        try {
          setKnowledge((await api.getKnowledgeStatus()) as KnowledgeStatus);
        } catch { /* ignore */ }
      })();
    }
  });

  async function handleRefresh() {
    setError(null); setRefreshBusy(true); setRefreshStage(null); setRefreshMsg(null);
    try {
      const ack = (await api.submitKnowledgeRefreshJob({ force })) as SubmitJobAck;
      setRefreshJobId(ack.jobId);
    } catch (e: unknown) { setError(String(e)); setRefreshBusy(false); setRefreshStage(null); }
  }

  async function handleRecheck() {
    setChecking(true); setError(null);
    try { setKnowledge((await api.checkKnowledgeStatus()) as KnowledgeStatus); }
    catch (e: unknown) { setError(String(e)); }
    finally { setChecking(false); }
  }

  async function handleExport() {
    setPackMsg(null); setPackBusy(true);
    try {
      const s = (await api.exportKnowledgePack(packPath, "AgentTheSpire-Rust")) as ExportPackStats;
      setPackMsg(`Exported ${s.gameFiles} game + ${s.baselibIncluded ? "baselib" : "no baselib"} → ${packPath}`);
    } catch (e: unknown) { setError(String(e)); }
    finally { setPackBusy(false); }
  }

  async function handleImport() {
    setPackMsg(null); setPackBusy(true);
    try {
      const s = (await api.importKnowledgePack(packPath, overwriteOnImport)) as ImportPackStats;
      setPackMsg(`Imported ${s.gameFilesWritten} files from ${packPath}`);
      void handleRecheck();
    } catch (e: unknown) { setError(String(e)); }
    finally { setPackBusy(false); }
  }

  const overallVariant =
    knowledge?.overall === "fresh" ? "ok" : knowledge?.overall === "stale" ? "warn" : "error";

  if (!__IS_TAURI__) {
    return <Card eyebrow="knowledge · sts2 sources" title="Knowledge" subtitle="desktop-only" />;
  }

  return (
    <Card
      eyebrow="knowledge · sts2 sources"
      title="Knowledge"
      actions={<Button size="sm" onClick={handleRecheck} disabled={checking}>{checking ? "Checking…" : "Re-check"}</Button>}
    >
      {error && <Notice variant="error" title={`Error: ${error}`} />}
      {!error && !knowledge && <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Loading…</p>}

      {knowledge && (
        <>
          <div className="flex items-center gap-3 flex-wrap mb-4">
            <Badge variant={overallVariant}>{knowledge.overall}</Badge>
            <span style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}>
              root <code>{knowledge.knowledgeRoot}</code>
            </span>
          </div>

          <div className="grid grid-cols-2 gap-3 mb-3">
            {[
              { label: "Game", obj: knowledge.game },
              { label: "BaseLib", obj: knowledge.baselib },
            ].map(({ label, obj }) => (
              <div key={label} className="p-3" style={{ background: "var(--paper)", border: "1px solid var(--rule-soft)", borderRadius: "4px" }}>
                <p style={{ fontFamily: '"JetBrains Mono", monospace', fontSize: "10px", letterSpacing: "0.16em", textTransform: "uppercase", color: "var(--ink-mute)", marginBottom: "6px" }}>
                  {label}
                </p>
                {obj ? (
                  <>
                    <p style={{ fontSize: "12px" }}><span style={{ color: "var(--ink-mute)" }}>mode </span>{obj.sourceMode}</p>
                    <p style={{ fontSize: "12px" }}><span style={{ color: "var(--ink-mute)" }}>decompiled </span>{String(obj.hasDecompiledSources)}</p>
                  </>
                ) : (
                  <p style={{ fontSize: "12px", color: "var(--ink-mute)" }}>absent</p>
                )}
              </div>
            ))}
          </div>

          {knowledge.embeddedTemplates && knowledge.embeddedTemplates.length > 0 && (
            <p style={{ fontSize: "11.5px", color: "var(--ink-mute)", marginBottom: "12px" }}>
              embedded templates: {knowledge.embeddedTemplates.join(", ")}
            </p>
          )}

          <CardSection title="刷新 & 导出 / 导入">
            <div className="flex items-center gap-3 mb-3">
              <label className="flex items-center gap-2" style={{ fontSize: "13px" }}>
                <input type="checkbox" checked={force} onChange={(e) => setForce(e.target.checked)} /> force
              </label>
              <Button variant="accent" onClick={() => void handleRefresh()} disabled={refreshBusy}>
                {refreshBusy ? (refreshStage ?? "Refreshing…") : "刷新知识库"}
              </Button>
              {refreshMsg && <span style={{ fontSize: "12px", color: "var(--ink-mute)" }}>{refreshMsg}</span>}
            </div>

            <div className="flex items-center gap-3 mb-3">
              <input value={packPath} onChange={(e) => setPackPath(e.target.value)} placeholder="导出 / 导入 .zip 路径" className="input-mono flex-1" />
              <label className="flex items-center gap-2" style={{ fontSize: "13px", whiteSpace: "nowrap" }}>
                <input type="checkbox" checked={overwriteOnImport} onChange={(e) => setOverwriteOnImport(e.target.checked)} /> overwrite
              </label>
              <Button onClick={() => void handleExport()} disabled={!packPath || packBusy}>Export</Button>
              <Button variant="accent" onClick={() => void handleImport()} disabled={!packPath || packBusy}>Import</Button>
            </div>
            {packMsg && <p style={{ fontSize: "12px", color: "var(--jade)" }}>{packMsg}</p>}
          </CardSection>
        </>
      )}
    </Card>
  );
}
