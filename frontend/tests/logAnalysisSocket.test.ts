import test from "node:test";
import assert from "node:assert/strict";

import { LogAnalysisSocket } from "../src/lib/log_analysis_ws.ts";

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  url: string;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send() {}

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  emitMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }
}

test.beforeEach(() => {
  MockWebSocket.instances = [];
  Object.assign(globalThis, {
    WebSocket: MockWebSocket,
    location: { host: "localhost:7860" },
  });
});

test("LogAnalysisSocket connects to analyze-log websocket endpoint", () => {
  new LogAnalysisSocket();

  assert.equal(MockWebSocket.instances[0]?.url, "ws://localhost:7860/api/ws/analyze-log");
});

test("LogAnalysisSocket dispatches log analysis events", () => {
  const socket = new LogAnalysisSocket();
  const received: Array<Record<string, unknown>> = [];

  socket.on("stage_update", (data) => {
    received.push(data as Record<string, unknown>);
  });
  socket.on("log_info", (data) => {
    received.push(data as Record<string, unknown>);
  });
  socket.on("done", (data) => {
    received.push(data as Record<string, unknown>);
  });

  MockWebSocket.instances[0].emitMessage({ event: "stage_update", scope: "text", stage: "reading_input", message: "正在读取日志" });
  MockWebSocket.instances[0].emitMessage({ event: "log_info", lines: 120 });
  MockWebSocket.instances[0].emitMessage({ event: "done", full: "分析完成" });

  assert.deepEqual(received, [
    { event: "stage_update", scope: "text", stage: "reading_input", message: "正在读取日志" },
    { event: "log_info", stage: "log_info", lines: 120 },
    { event: "done", stage: "done", full: "分析完成" },
  ]);
});
