// RunsCard 主壳 —— 拼接 RunsSubmitForm + RunsList。
//
// 顶层只持有 error；列表自管 list state，监听 run-progress 自动 refresh；
// 表单负责所有 kind 字段。提交完调 listRef.current.refresh() 立刻刷新。

import { useRef, useState } from "react";
import { RunsList, type RunsListHandle } from "@/components/RunsList";
import { RunsSubmitForm } from "@/components/RunsSubmitForm";
import { Card, Notice } from "@/components/ui";

export function RunsCard() {
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<RunsListHandle>(null);

  if (!__IS_TAURI__) {
    return (
      <Card
        eyebrow="background · runs"
        title="Runs"
        subtitle="Platform runs are desktop-only for now. Web sqlx repository lands in stage 3.1a."
      />
    );
  }

  return (
    <Card
      eyebrow="background · runs"
      title="Runs"
      subtitle="submit any handler — text_generate / asset / build / package / log / truth snapshot"
    >
      {error && (
        <div data-testid="run-error">
          <Notice variant="error" title={`Error: ${error}`} className="mb-3" />
        </div>
      )}

      <details open className="mb-4">
        <summary
          className="cursor-pointer mb-3"
          style={{
            fontFamily: '"JetBrains Mono", monospace',
            fontSize: "10.5px",
            letterSpacing: "0.16em",
            textTransform: "uppercase",
            color: "var(--ink-mute)",
          }}
        >
          Submit new run
        </summary>
        <RunsSubmitForm
          onSubmitted={(_runId) => {
            setError(null);
            void listRef.current?.refresh();
          }}
          onError={(msg) => setError(msg)}
        />
      </details>

      <RunsList ref={listRef} onError={(msg) => setError(msg)} />
    </Card>
  );
}
