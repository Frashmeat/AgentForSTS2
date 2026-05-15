// JobsCard 主壳 —— 拼接 JobsSubmitForm + JobsList。
//
// 顶层只持有 error；列表自管 list state，监听 job-progress 自动 refresh；
// 表单负责所有 kind 字段。提交完调 listRef.current.refresh() 立刻刷新。

import { useRef, useState } from "react";
import { JobsList, type JobsListHandle } from "@/components/JobsList";
import { JobsSubmitForm } from "@/components/JobsSubmitForm";
import { Card, Notice } from "@/components/ui";

export function JobsCard() {
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<JobsListHandle>(null);

  if (!__IS_TAURI__) {
    return (
      <Card
        eyebrow="background · jobs"
        title="Jobs"
        subtitle="Platform jobs are desktop-only for now. Web sqlx repository lands in stage 3.1a."
      />
    );
  }

  return (
    <Card
      eyebrow="background · jobs"
      title="Jobs"
      subtitle="submit any handler — text_generate / asset / build / package / log / knowledge"
    >
      {error && <Notice variant="error" title={`Error: ${error}`} className="mb-3" />}

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
          Submit new job
        </summary>
        <JobsSubmitForm
          onSubmitted={(_jobId) => {
            setError(null);
            void listRef.current?.refresh();
          }}
          onError={(msg) => setError(msg)}
        />
      </details>

      <JobsList ref={listRef} onError={(msg) => setError(msg)} />
    </Card>
  );
}
