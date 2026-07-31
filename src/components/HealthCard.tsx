import { useEffect, useState } from "react";
import { Badge, Card, CardSection, KV, KVList, Notice } from "@/components/ui";
import { api } from "@/services/api";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { toActionableFailure } from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
import type { HealthReport } from "@/services/tauriApi";

function ReadyDot({ ok, label, title }: { ok: boolean; label: string; title?: string }) {
  return (
    <li className="flex items-baseline gap-2">
      <span
        title={title}
        style={{
          color: ok ? "var(--jade)" : "var(--ink-faint)",
          fontFamily: '"JetBrains Mono", monospace',
          fontSize: "12px",
          width: "12px",
          textAlign: "center",
          flexShrink: 0,
        }}
      >
        {ok ? "✓" : "○"}
      </span>
      <span style={{ fontSize: "13px" }}>{label}</span>
    </li>
  );
}

export function HealthCard() {
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [error, setError] = useState<ActionableFailure | null>(null);

  useEffect(() => {
    (api.getHealth() as Promise<HealthReport>)
      .then(setHealth)
      .catch((e: unknown) => setError(toActionableFailure(e)));
  }, []);

  const statusVariant =
    health?.status === "ok"
      ? "ok"
      : health?.status === "degraded"
        ? "warn"
        : "muted";

  return (
    <Card
      eyebrow="diagnostics · runtime"
      title="Health"
      actions={
        health && (
          <>
            <Badge variant={statusVariant}>{health.status}</Badge>
            <Badge variant="muted">role · {health.role}</Badge>
            <Badge variant="muted">core · {health.coreVersion}</Badge>
          </>
        )
      }
    >
      <ActionableErrorNotice failure={error} />
      {!error && !health && (
        <p style={{ color: "var(--ink-mute)", fontSize: "13px" }}>Loading…</p>
      )}

      {health && (
        <>
          <CardSection title="Config">
            <KVList>
              <KV k="path">
                <code>{health.config.path ?? "<none>"}</code>
              </KV>
              <KV k="file present">{String(health.config.filePresent)}</KV>
              <KV k="loaded">{String(health.config.loaded)}</KV>
            </KVList>
            {health.config.errors.length > 0 && (
              <Notice
                variant="warn"
                title="Config issues"
                className="mt-3"
              >
                <ul className="space-y-0.5">
                  {health.config.errors.map((e, i) => (
                    <li key={i} style={{ fontSize: "12px" }}>• {e}</li>
                  ))}
                </ul>
              </Notice>
            )}
          </CardSection>

          <CardSection title="Readiness">
            <ul className="space-y-1.5">
              <ReadyDot ok={health.readiness.llmConfigured} label="LLM api_key 配置" />
              <ReadyDot
                ok={health.readiness.imageGenConfigured}
                label="image_gen api_key 配置（asset_generate 需要）"
              />
              <ReadyDot
                ok={health.readiness.activeProjectOpen}
                label="工程文件夹已打开"
              />
              <ReadyDot
                ok={health.readiness.imageProcReady}
                label="ML 背景去除就绪（否则走启发式 fallback）"
                title="cargo build --features ml-rembg 启用 + 模型加载成功"
              />
              <ReadyDot
                ok={health.readiness.truthSnapshotReady}
                label="Verified Truth Snapshot 就绪"
                title="生成任务只读取活动工程 Game Pack 的 verified current Snapshot"
              />
              <ReadyDot ok={health.readiness.queueWorkerReady} label="后台任务 worker" />
            </ul>
          </CardSection>
        </>
      )}
    </Card>
  );
}
