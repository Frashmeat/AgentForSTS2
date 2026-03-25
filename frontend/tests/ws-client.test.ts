import test from "node:test";
import assert from "node:assert/strict";

import { normalizeEvent } from "../src/shared/ws/events.ts";
import { WorkflowClient } from "../src/shared/ws/client.ts";

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((event?: { wasClean?: boolean; code?: number }) => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  url: string;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send() {}

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ wasClean: true, code: 1000 });
  }

  emitOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  emitClose(event: { wasClean?: boolean; code?: number } = {}) {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.(event);
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

test("normalizes workflow events from websocket payloads", () => {
  const event = normalizeEvent({ stage: "done" });
  assert.equal(event.stage, "done");
});

test("workflow client waits for open", async () => {
  const client = new WorkflowClient("/api/ws/create");
  const pending = client.waitOpen();

  MockWebSocket.instances[0].emitOpen();

  await pending;
});

test("workflow client dispatches normalized events", async () => {
  const client = new WorkflowClient("/api/ws/create");
  let received: { event: string; stage?: string } | null = null;
  client.on("done", (data) => {
    received = data as { event: string; stage?: string };
  });

  MockWebSocket.instances[0].emitMessage({ event: "done", success: true });

  assert.deepEqual(received, { event: "done", stage: "done", success: true });
});
