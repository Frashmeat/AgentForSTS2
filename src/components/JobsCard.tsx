// JobsCard 主壳 —— 拼接 JobsSubmitForm + JobsList。
//
// 顶层只持有 error；列表自管 list state，监听 job-progress 自动 refresh；
// 表单负责所有 kind 字段。提交完调 listRef.current.refresh() 立刻刷新。

import { useRef, useState } from "react";
import { JobsList, type JobsListHandle } from "@/components/JobsList";
import { JobsSubmitForm } from "@/components/JobsSubmitForm";

export function JobsCard() {
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<JobsListHandle>(null);

  if (!__IS_TAURI__) {
    return (
      <section className="rounded border border-muted/30 p-4">
        <h2 className="text-lg font-medium mb-2">Jobs</h2>
        <p className="text-muted text-sm">
          Platform jobs are desktop-only for now. Web sqlx repository lands in
          stage 3.1a.
        </p>
      </section>
    );
  }

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-medium">Jobs — submit any handler</h2>
      </div>

      {error && <p className="text-red-500 text-sm mb-2">Error: {error}</p>}

      <details open className="mb-4">
        <summary className="cursor-pointer text-sm font-medium mb-2">
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
    </section>
  );
}
