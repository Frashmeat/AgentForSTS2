import { useEffect, useState } from "react";

import {
  getRefreshKnowledgeTask,
  loadKnowledgeStatus,
  startRefreshKnowledgeTask,
  type KnowledgeStatus,
} from "../../shared/api/index.ts";

function createRefreshingKnowledgeStatus(previous: KnowledgeStatus | null): KnowledgeStatus {
  return previous
    ? { ...previous, status: "refreshing" }
    : {
        status: "refreshing",
        generated_at: null,
        checked_at: null,
        warnings: [],
        game: {},
        baselib: {},
      };
}

export function useKnowledgeStatusFlow() {
  const [knowledgeStatus, setKnowledgeStatus] = useState<KnowledgeStatus | null>(null);
  const [knowledgeRefreshTaskId, setKnowledgeRefreshTaskId] = useState("");

  useEffect(() => {
    void loadKnowledgeStatus()
      .then((status) => {
        setKnowledgeStatus(status);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!knowledgeRefreshTaskId) {
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function pollKnowledgeRefresh() {
      try {
        const snapshot = await getRefreshKnowledgeTask(knowledgeRefreshTaskId);
        if (cancelled) {
          return;
        }
        if (snapshot.status === "running" || snapshot.status === "pending") {
          timer = setTimeout(pollKnowledgeRefresh, 800);
          return;
        }
        const status = await loadKnowledgeStatus();
        if (!cancelled) {
          setKnowledgeStatus(status);
        }
        setKnowledgeRefreshTaskId("");
      } catch {
        if (cancelled) {
          return;
        }
        setKnowledgeRefreshTaskId("");
      }
    }

    void pollKnowledgeRefresh();

    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [knowledgeRefreshTaskId]);

  async function handleRefreshKnowledge() {
    setKnowledgeStatus((previous) => createRefreshingKnowledgeStatus(previous));
    try {
      const task = await startRefreshKnowledgeTask();
      setKnowledgeRefreshTaskId(task.task_id);
    } catch {
      try {
        const status = await loadKnowledgeStatus();
        setKnowledgeStatus(status);
      } catch {
        setKnowledgeStatus((previous) => previous ? { ...previous, status: "error" } : previous);
      }
    }
  }

  return {
    knowledgeStatus,
    handleRefreshKnowledge,
  };
}
