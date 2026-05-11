// 双壳 LLM 流式抽象。
//
// 桌面端：调 src-tauri::commands::llm::llm_start_stream，监听 `llm-stream` 事件
// Web 端：fetch POST /api/llm/stream，解析 SSE 流
//
// 上层只用 streamLlmCompletion(req, callbacks)，返回 cancel handle。

import type {
  CompletionRequest,
  FinishReason,
  StreamEvent,
  Usage,
} from "./tauriApi";

export interface StreamCallbacks {
  onStart?: (model: string) => void;
  onDelta: (text: string) => void;
  onEnd?: (finishReason: FinishReason, usage: Usage) => void;
  onError: (message: string) => void;
}

export interface StreamHandle {
  cancel: () => void;
}

/**
 * 后端事件 envelope（与 ats-web routes/llm.rs::SsePayload 和
 * src-tauri commands/llm.rs::StreamPayload 共用）。
 */
type Envelope =
  | { type: "event"; request_id?: string; event: StreamEvent }
  | { type: "error"; request_id?: string; message: string }
  | { type: "done"; request_id?: string };

function dispatch(envelope: Envelope, callbacks: StreamCallbacks): boolean {
  if (envelope.type === "event") {
    const ev = envelope.event;
    if (ev.kind === "start") {
      callbacks.onStart?.(ev.model);
    } else if (ev.kind === "delta") {
      callbacks.onDelta(ev.text);
    } else if (ev.kind === "end") {
      callbacks.onEnd?.(ev.finishReason, ev.usage);
      return true;
    }
  } else if (envelope.type === "error") {
    callbacks.onError(envelope.message);
    return true;
  } else if (envelope.type === "done") {
    return true;
  }
  return false;
}

export function streamLlmCompletion(
  request: CompletionRequest,
  callbacks: StreamCallbacks,
): StreamHandle {
  if (__IS_TAURI__) {
    return startTauriStream(request, callbacks);
  }
  return startWebStream(request, callbacks);
}

// -------- Tauri --------

function startTauriStream(
  request: CompletionRequest,
  callbacks: StreamCallbacks,
): StreamHandle {
  const requestId =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `req-${Date.now()}-${Math.random()}`;
  let unlisten: (() => void) | null = null;
  let cancelled = false;

  void (async () => {
    const { listen } = await import("@tauri-apps/api/event");
    const { invoke } = await import("@tauri-apps/api/core");
    if (cancelled) return;

    unlisten = await listen<Envelope & { request_id?: string }>(
      "llm-stream",
      (event) => {
        const payload = event.payload;
        if (payload.request_id && payload.request_id !== requestId) {
          return;
        }
        const terminal = dispatch(payload, callbacks);
        if (terminal && unlisten) {
          unlisten();
          unlisten = null;
        }
      },
    );

    if (cancelled) {
      unlisten?.();
      unlisten = null;
      return;
    }

    try {
      await invoke("llm_start_stream", { requestId, request });
    } catch (e) {
      callbacks.onError(String(e));
      unlisten?.();
      unlisten = null;
    }
  })();

  return {
    cancel: () => {
      cancelled = true;
      unlisten?.();
      unlisten = null;
    },
  };
}

// -------- Web --------

function startWebStream(
  request: CompletionRequest,
  callbacks: StreamCallbacks,
): StreamHandle {
  const controller = new AbortController();

  void (async () => {
    try {
      const response = await fetch("/api/llm/stream", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(request),
        signal: controller.signal,
      });
      if (!response.ok || !response.body) {
        callbacks.onError(
          `HTTP ${response.status} ${response.statusText || "(no body)"}`,
        );
        return;
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      let terminal = false;

      while (!terminal) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        // SSE 事件由空行分隔。每事件含若干 `data: ...` 行；keep-alive 注释以 `:` 开头跳过。
        let separator = buffer.indexOf("\n\n");
        while (separator !== -1) {
          const block = buffer.slice(0, separator);
          buffer = buffer.slice(separator + 2);
          const dataLines: string[] = [];
          for (const line of block.split("\n")) {
            if (line.startsWith(":")) continue; // ping 注释
            if (line.startsWith("data:")) {
              dataLines.push(line.slice(5).trimStart());
            }
          }
          if (dataLines.length > 0) {
            const dataStr = dataLines.join("\n");
            try {
              const envelope = JSON.parse(dataStr) as Envelope;
              if (dispatch(envelope, callbacks)) {
                terminal = true;
                break;
              }
            } catch (err) {
              callbacks.onError(`failed to parse SSE payload: ${String(err)}`);
              terminal = true;
              break;
            }
          }
          separator = buffer.indexOf("\n\n");
        }
      }
    } catch (e) {
      if (controller.signal.aborted) {
        // 用户主动 cancel 不报错
        return;
      }
      callbacks.onError(String(e));
    }
  })();

  return {
    cancel: () => controller.abort(),
  };
}
