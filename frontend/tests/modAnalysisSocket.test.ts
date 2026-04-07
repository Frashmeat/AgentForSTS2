import test from "node:test";
import assert from "node:assert/strict";

import { ModAnalysisSocket } from "../src/lib/mod_analysis_ws.ts";

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

test("ModAnalysisSocket connects to analyze-mod websocket endpoint", () => {
  new ModAnalysisSocket();

  assert.equal(MockWebSocket.instances[0]?.url, "ws://localhost:7860/api/ws/analyze-mod");
});

test("ModAnalysisSocket dispatches mod analysis events", () => {
  const socket = new ModAnalysisSocket();
  const received: Array<Record<string, unknown>> = [];

  socket.on("stage_update", (data) => {
    received.push(data as Record<string, unknown>);
  });
  socket.on("scan_info", (data) => {
    received.push(data as Record<string, unknown>);
  });
  socket.on("stream", (data) => {
    received.push(data as Record<string, unknown>);
  });
  socket.on("done", (data) => {
    received.push(data as Record<string, unknown>);
  });

  MockWebSocket.instances[0].emitMessage({ event: "stage_update", scope: "text", stage: "reading_input", message: "正在扫描项目" });
  MockWebSocket.instances[0].emitMessage({ event: "scan_info", files: 18 });
  MockWebSocket.instances[0].emitMessage({
    event: "stream",
    chunk: "分析中",
    source: "analysis",
    channel: "raw",
    model: "claude-sonnet-4-6",
  });
  MockWebSocket.instances[0].emitMessage({ event: "done", full: "分析完成" });

  assert.deepEqual(received, [
    { event: "stage_update", scope: "text", stage: "reading_input", message: "正在扫描项目" },
    { event: "scan_info", stage: "scan_info", files: 18 },
    { event: "stream", stage: "stream", chunk: "分析中", source: "analysis", channel: "raw", model: "claude-sonnet-4-6" },
    { event: "done", stage: "done", full: "分析完成" },
  ]);
});
