import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "@/services/api";
import type { KnowledgeStatus } from "@/services/tauriApi";

export function KnowledgePill() {
  const [status, setStatus] = useState<KnowledgeStatus | null>(null);
  const nav = useNavigate();

  useEffect(() => {
    if (!__IS_TAURI__) return;
    (api.getKnowledgeStatus() as Promise<KnowledgeStatus>)
      .then(setStatus)
      .catch(() => {});
    // Poll every 30s
    const i = setInterval(() => {
      (api.getKnowledgeStatus() as Promise<KnowledgeStatus>)
        .then(setStatus)
        .catch(() => {});
    }, 30000);
    return () => clearInterval(i);
  }, []);

  if (!__IS_TAURI__) return null;

  const overall = status?.overall ?? "missing";
  
  const color = overall === "fresh" ? "#16a34a" : overall === "stale" ? "#d97706" : "#dc2626";

  return (
    <button
      onClick={() => nav("/system?tab=ops")}
      title={`知识库: ${overall}`}
      style={{
        display: "inline-flex", alignItems: "center", gap: "5px",
        padding: "3px 10px", borderRadius: "4px", border: "none", cursor: "pointer",
        fontSize: "11.5px", fontFamily: '"JetBrains Mono", monospace',
        background: "var(--paper-soft)", color: color,
      }}
    >
      <span>🧠</span>
      <span>{overall === "fresh" ? "知识库" : overall === "stale" ? "知识库(旧)" : "知识库 ✗"}</span>
    </button>
  );
}
