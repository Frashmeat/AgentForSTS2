// 统一的 run-progress 事件监听 hook。
// 消灭 6 个组件里重复的 listen + cleanup 样板。
//
// useRunProgress: 按 runId 过滤，只回调匹配的事件。
// useAllRunProgress: 不过滤，所有 run 的事件都回调。

import { useEffect, useRef } from "react";
import type { RunProgressEvent } from "@/services/tauriApi";

/**
 * 监听指定 runId 的 progress 事件。runId 通过 ref 传入，避免 useEffect
 * 重跑——事件监听只建立一次，回调始终拿到最新 closure。
 */
export function useRunProgress(
  runIdRef: React.MutableRefObject<string | null>,
  onEvent: (ev: RunProgressEvent) => void,
): void {
  const handlerRef = useRef(onEvent);
  handlerRef.current = onEvent;

  useEffect(() => {
    if (!__IS_TAURI__) return;

    const unlisten = { current: null as (() => void) | null };

    // Dynamic import justified: @tauri-apps/api/event 是 Tauri 专属模块，
    // Web 构建中不存在对应的 IPC bridge。
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<RunProgressEvent>("run-progress", (e) => {
        if (e.payload.runId === runIdRef.current) {
          handlerRef.current(e.payload);
        }
      });
      unlisten.current = stop;
    })();

    return () => {
      unlisten.current?.();
      unlisten.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}

/**
 * 监听所有 run 的 progress 事件（不过滤 runId）。
 * 适用场景：RunsList 实时刷新列表。
 */
export function useAllRunProgress(
  onEvent: (ev: RunProgressEvent) => void,
): void {
  const handlerRef = useRef(onEvent);
  handlerRef.current = onEvent;

  useEffect(() => {
    if (!__IS_TAURI__) return;

    const unlisten = { current: null as (() => void) | null };

    // Dynamic import justified: @tauri-apps/api/event 是 Tauri 专属模块。
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen<RunProgressEvent>("run-progress", (e) => {
        handlerRef.current(e.payload);
      });
      unlisten.current = stop;
    })();

    return () => {
      unlisten.current?.();
      unlisten.current = null;
    };
  }, []);
}
