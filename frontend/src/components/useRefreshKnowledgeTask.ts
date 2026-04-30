// SettingsPanel 的"知识库更新后台任务"状态机封装。
// 与 useDetectAppPathsTask 同模式：保留 module-level retained id；mount 时恢复未结束任务；
// 轮询；3.2s 自动淡出；handleCheck / handleRefresh 两个动作。
//
// 与 detection 不同的是：知识库还有一个独立的 "checking" 状态（手动 checkKnowledgeStatus）
// 与 polling 状态共享 step/notes/error 字段，busy 通过 checking||taskId 推算。

import { useEffect, useState, type MutableRefObject } from "react";

import {
  checkKnowledgeStatus,
  getLatestRefreshKnowledgeTask,
  getRefreshKnowledgeTask,
  loadKnowledgeStatus,
  startRefreshKnowledgeTask,
  type KnowledgeStatus,
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";

let retainedKnowledgeTaskId = "";

function isActiveTask(status: string): boolean {
  return status === "pending" || status === "running";
}

export interface UseRefreshKnowledgeTaskResult {
  knowledgeStatus: KnowledgeStatus | null;
  knowledgeChecking: boolean;
  knowledgeBusy: boolean;
  knowledgeTaskId: string;
  knowledgeStep: string;
  knowledgeNotes: string[];
  knowledgeError: string;
  handleCheck: () => Promise<void>;
  handleRefresh: () => Promise<void>;
}

export interface UseRefreshKnowledgeTaskOptions {
  mountedRef: MutableRefObject<boolean>;
  onKnowledgeStatusChange?: (status: KnowledgeStatus) => void;
}

export function useRefreshKnowledgeTask({
  mountedRef,
  onKnowledgeStatusChange,
}: UseRefreshKnowledgeTaskOptions): UseRefreshKnowledgeTaskResult {
  const [knowledgeStatus, setKnowledgeStatus] = useState<KnowledgeStatus | null>(null);
  const [taskIdState, setTaskIdState] = useState(retainedKnowledgeTaskId);
  const [knowledgeChecking, setKnowledgeChecking] = useState(false);
  const [knowledgeStep, setKnowledgeStep] = useState("");
  const [knowledgeNotes, setKnowledgeNotes] = useState<string[]>([]);
  const [knowledgeError, setKnowledgeError] = useState("");

  const knowledgeBusy = knowledgeChecking || Boolean(taskIdState);

  function setTaskId(value: string) {
    retainedKnowledgeTaskId = value;
    if (mountedRef.current) {
      setTaskIdState(value);
    }
  }

  // mount-only：拉一次状态 + 尝试恢复未结束任务
  useEffect(() => {
    loadKnowledgeStatus()
      .then((status) => {
        if (!mountedRef.current) {
          return;
        }
        setKnowledgeStatus(status);
        onKnowledgeStatusChange?.(status);
      })
      .catch((error) => {
        if (!mountedRef.current) {
          return;
        }
        setKnowledgeError(resolveErrorMessage(error));
      });

    getLatestRefreshKnowledgeTask()
      .then((task) => {
        if (!mountedRef.current || !task || retainedKnowledgeTaskId || !isActiveTask(task.status)) {
          return;
        }
        setKnowledgeError("");
        setKnowledgeStep(task.current_step || "");
        setKnowledgeNotes(task.notes ?? []);
        setTaskId(task.task_id);
      })
      .catch(() => {
        // 恢复入口失败不阻塞 SettingsPanel 初始化
      });
    // mount-only：onKnowledgeStatusChange 是父级 callback 故意省略避免重复触发
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 轮询任务直到 terminal
  useEffect(() => {
    if (!taskIdState) {
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function pollRefreshTask() {
      try {
        const snapshot = await getRefreshKnowledgeTask(taskIdState);
        if (cancelled) {
          return;
        }

        setKnowledgeStep(snapshot.current_step || "");
        setKnowledgeNotes(snapshot.notes ?? []);

        if (snapshot.status === "running" || snapshot.status === "pending") {
          timer = setTimeout(pollRefreshTask, 800);
          return;
        }

        if (snapshot.status === "failed") {
          setKnowledgeError(snapshot.error?.trim() || "知识库更新失败");
        }

        const status = await loadKnowledgeStatus();
        if (!cancelled && mountedRef.current) {
          setKnowledgeStatus(status);
          onKnowledgeStatusChange?.(status);
        }
        setTaskId("");
      } catch (error) {
        if (cancelled || !mountedRef.current) {
          return;
        }
        setKnowledgeError(resolveErrorMessage(error));
        setTaskId("");
      }
    }

    void pollRefreshTask();

    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
    // 仅由 taskIdState 驱动；onKnowledgeStatusChange 故意省略避免父级 callback 引用变化触发重启
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskIdState]);

  // 任务结束、无 error/notes 时，3.2s 后自动淡出
  useEffect(() => {
    if (knowledgeChecking || taskIdState || knowledgeError || knowledgeNotes.length === 0) {
      return;
    }

    const timer = setTimeout(() => {
      setKnowledgeStep("");
      setKnowledgeNotes([]);
    }, 3200);
    return () => clearTimeout(timer);
  }, [knowledgeChecking, taskIdState, knowledgeError, knowledgeNotes]);

  async function handleCheck() {
    if (knowledgeBusy) {
      return;
    }
    setKnowledgeChecking(true);
    setKnowledgeError("");
    setKnowledgeStep("检查知识库状态");
    setKnowledgeNotes([]);
    try {
      const status = await checkKnowledgeStatus();
      if (!mountedRef.current) {
        return;
      }
      setKnowledgeStatus(status);
      onKnowledgeStatusChange?.(status);
      setKnowledgeNotes(status.warnings ?? []);
      setKnowledgeStep("");
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setKnowledgeError(resolveErrorMessage(error));
      setKnowledgeStep("检查失败");
    } finally {
      if (mountedRef.current) {
        setKnowledgeChecking(false);
      }
    }
  }

  async function handleRefresh() {
    if (knowledgeBusy) {
      return;
    }
    setKnowledgeError("");
    setKnowledgeNotes([]);
    setKnowledgeStep("准备启动知识库更新任务");
    try {
      const task = await startRefreshKnowledgeTask();
      if (!mountedRef.current) {
        retainedKnowledgeTaskId = task.task_id;
        return;
      }
      setTaskId(task.task_id);
      setKnowledgeStep(task.current_step || "更新中");
      setKnowledgeNotes(task.notes ?? []);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setKnowledgeError(resolveErrorMessage(error));
    }
  }

  return {
    knowledgeStatus,
    knowledgeChecking,
    knowledgeBusy,
    knowledgeTaskId: taskIdState,
    knowledgeStep,
    knowledgeNotes,
    knowledgeError,
    handleCheck,
    handleRefresh,
  };
}
