// SettingsPanel 的"路径检测后台任务"状态机封装。
// 责任：
//   - 启动 / 取消 / 轮询 detect_paths 后台任务
//   - mount 时尝试恢复最近一次未结束任务（getLatestDetectAppPathsTask）
//   - 任务结束后保留 notes 3.2s 再自动清掉，让用户看见结果
//   - 跨组件实例保留 task_id（module-level retained id），防止 SettingsPanel 卸载/重挂时丢任务
//
// 输出：当前状态 + start() / cancel() 两个动作。

import { useEffect, useState, type MutableRefObject } from "react";

import {
  cancelDetectAppPathsTask,
  getDetectAppPathsTask,
  getLatestDetectAppPathsTask,
  startDetectAppPaths,
  type DetectPathsTaskResult,
} from "../shared/api/index.ts";
import { resolveErrorMessage } from "../shared/error.ts";

let retainedDetectionTaskId = "";

function isActiveTask(status: string): boolean {
  return status === "pending" || status === "running";
}

export interface UseDetectAppPathsTaskResult {
  detecting: boolean;
  taskId: string;
  step: string;
  notes: string[];
  start: () => Promise<void>;
  cancel: () => Promise<void>;
  setNotes: (notes: string[] | ((prev: string[]) => string[])) => void;
}

export interface UseDetectAppPathsTaskOptions {
  mountedRef: MutableRefObject<boolean>;
  onSnapshot?: (snapshot: DetectPathsTaskResult) => void;
}

export function useDetectAppPathsTask({
  mountedRef,
  onSnapshot,
}: UseDetectAppPathsTaskOptions): UseDetectAppPathsTaskResult {
  const [detecting, setDetecting] = useState(Boolean(retainedDetectionTaskId));
  const [taskIdState, setTaskIdState] = useState(retainedDetectionTaskId);
  const [step, setStep] = useState("");
  const [notes, setNotes] = useState<string[]>([]);

  function setTaskId(value: string) {
    retainedDetectionTaskId = value;
    if (mountedRef.current) {
      setTaskIdState(value);
    }
  }

  // mount-only：尝试恢复最近一次未结束任务
  useEffect(() => {
    getLatestDetectAppPathsTask()
      .then((task) => {
        if (!mountedRef.current || !task || retainedDetectionTaskId || !isActiveTask(task.status)) {
          return;
        }
        setDetecting(true);
        setStep(task.current_step || "");
        setNotes(task.notes ?? []);
        setTaskId(task.task_id);
      })
      .catch(() => {
        // 恢复入口失败不阻塞 SettingsPanel 初始化
      });
    // mount-only：仅初次尝试恢复，mountedRef 是 ref 永不进 deps
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // taskId 变化时启动轮询
  useEffect(() => {
    if (!taskIdState) {
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function pollTask() {
      try {
        const snapshot = await getDetectAppPathsTask(taskIdState);
        if (cancelled || !mountedRef.current) {
          return;
        }

        setStep(snapshot.current_step || "");
        setNotes(snapshot.notes ?? []);
        onSnapshot?.(snapshot);

        if (snapshot.status === "running" || snapshot.status === "pending") {
          timer = setTimeout(pollTask, 500);
          return;
        }

        if (snapshot.status === "failed") {
          const message = snapshot.error?.trim() || "检测失败，请使用右侧选择按钮手动指定路径";
          setNotes((prev) => [...(snapshot.notes ?? prev), `检测失败：${message}`]);
        }

        setDetecting(false);
        setTaskId("");
      } catch (error) {
        if (cancelled) {
          return;
        }
        setDetecting(false);
        setTaskId("");
        setStep("");
        setNotes([`检测失败：${resolveErrorMessage(error)}`]);
      }
    }

    void pollTask();

    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
    // onSnapshot 是父级 callback，故意不进 deps 避免父级引用变化触发重启轮询
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskIdState]);

  // 任务结束、无失败 notes 时，3.2s 后自动淡出
  useEffect(() => {
    if (detecting || taskIdState || notes.length === 0) {
      return;
    }
    const hasFailure = notes.some((note) => note.includes("失败"));
    if (hasFailure) {
      return;
    }

    const timer = setTimeout(() => {
      setStep("");
      setNotes([]);
    }, 3200);
    return () => clearTimeout(timer);
  }, [detecting, taskIdState, notes]);

  async function start() {
    setDetecting(true);
    setStep("准备启动检测任务");
    setNotes([]);
    try {
      const task = await startDetectAppPaths();
      if (!mountedRef.current) {
        retainedDetectionTaskId = task.task_id;
        return;
      }
      setTaskId(task.task_id);
      setStep(task.current_step || "检测中");
      setNotes(task.notes ?? []);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setStep("");
      setNotes([`检测失败：${resolveErrorMessage(error)}`]);
      setDetecting(false);
    }
  }

  async function cancel() {
    if (!taskIdState) {
      return;
    }
    try {
      const snapshot = await cancelDetectAppPathsTask(taskIdState);
      if (!mountedRef.current) {
        return;
      }
      setStep(snapshot.current_step || "检测已取消");
      setNotes(snapshot.notes ?? ["检测已取消"]);
    } catch (error) {
      if (!mountedRef.current) {
        return;
      }
      setNotes([`取消检测失败：${resolveErrorMessage(error)}`]);
    }
  }

  return {
    detecting,
    taskId: taskIdState,
    step,
    notes,
    start,
    cancel,
    setNotes,
  };
}
