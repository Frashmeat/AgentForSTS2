import { useEffect, useState } from "react";
import { Database } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { api } from "@/services/api";
import type { TruthSnapshotStatus } from "@/services/tauriApi";
import { useProjectStore } from "@/stores/project";

export function KnowledgePill() {
  const [status, setStatus] = useState<TruthSnapshotStatus | null>(null);
  const navigate = useNavigate();
  const projectPath = useProjectStore((state) => state.project?.path);

  useEffect(() => {
    if (!__IS_TAURI__) return;
    if (!projectPath) {
      setStatus(null);
      return;
    }
    const refresh = () => {
      (api.getTruthSnapshotStatus() as Promise<TruthSnapshotStatus>)
        .then(setStatus)
        .catch(() => setStatus(null));
    };
    refresh();
    const interval = setInterval(refresh, 60_000);
    return () => clearInterval(interval);
  }, [projectPath]);

  if (!__IS_TAURI__) return null;

  const state = status?.state ?? "missing";
  const color =
    state === "ready" ? "#15803d" : state === "invalid" ? "#b91c1c" : "#b45309";

  return (
    <button
      onClick={() => navigate("/system?tab=ops")}
      title={`Truth Snapshot: ${state}`}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "5px",
        padding: "3px 8px",
        borderRadius: "4px",
        border: "none",
        cursor: "pointer",
        fontSize: "11.5px",
        fontFamily: '"JetBrains Mono", monospace',
        background: "var(--paper-soft)",
        color,
      }}
    >
      <Database size={13} />
      <span>{state === "ready" ? "Snapshot" : `Snapshot ${state}`}</span>
    </button>
  );
}
