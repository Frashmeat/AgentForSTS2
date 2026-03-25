import test from "node:test";
import assert from "node:assert/strict";

import { BatchSocket } from "../src/lib/batch_ws.ts";

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

  emitOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
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

test("BatchSocket connects to batch websocket endpoint", () => {
  new BatchSocket();

  assert.equal(MockWebSocket.instances[0]?.url, "ws://localhost:7860/api/ws/batch");
});

test("BatchSocket dispatches typed batch events", () => {
  const socket = new BatchSocket();
  let received: { event: string; stage?: string; success_count?: number } | null = null;

  socket.on("batch_done", (data) => {
    received = data as { event: string; stage?: string; success_count?: number };
  });

  MockWebSocket.instances[0].emitMessage({ event: "batch_done", success_count: 2, error_count: 0 });

  assert.deepEqual(received, { event: "batch_done", stage: "batch_done", success_count: 2, error_count: 0 });
});
