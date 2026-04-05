import test from "node:test";
import assert from "node:assert/strict";

import { WorkflowSocket } from "../src/lib/ws.ts";
import { BuildDeploySocket } from "../src/lib/build_deploy_ws.ts";

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
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

  emitError() {
    this.onerror?.();
  }

  emitClose() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }
}

function rejectIfPending<T>(promise: Promise<T>, label: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => {
      setTimeout(() => reject(new Error(`Timed out waiting for ${label}`)), 50);
    }),
  ]);
}

test.beforeEach(() => {
  MockWebSocket.instances = [];
  Object.assign(globalThis, {
    WebSocket: MockWebSocket,
    location: { host: "localhost:7860" },
  });
});

test("waitOpen 在连接错误时 reject", async () => {
  const socket = new WorkflowSocket();
  const pending = socket.waitOpen();

  MockWebSocket.instances[0].emitError();

  await assert.rejects(
    rejectIfPending(pending, "waitOpen rejection on error"),
    /WebSocket connection failed/
  );
});

test("waitOpen 在连接提前关闭时 reject", async () => {
  const socket = new WorkflowSocket();
  const pending = socket.waitOpen();

  MockWebSocket.instances[0].emitClose();

  await assert.rejects(
    rejectIfPending(pending, "waitOpen rejection on close"),
    /WebSocket connection closed/
  );
});

test("waitOpen 在连接成功时 resolve", async () => {
  const socket = new WorkflowSocket();
  const pending = socket.waitOpen();

  MockWebSocket.instances[0].emitOpen();

  await pending;
});

test("workflow socket uses configured workstation websocket base", () => {
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_WS_BASES__: {
      workstation: "wss://workstation.example.com",
    },
  });

  new WorkflowSocket();

  assert.equal(MockWebSocket.instances[0].url, "wss://workstation.example.com/api/ws/create");
});

test("workstation websocket clients fail loudly without workstation config on independent frontend", () => {
  const runtimeGlobals = globalThis as typeof globalThis & {
    __AGENT_THE_SPIRE_WS_BASES__?: unknown;
    location?: { host?: string; href?: string };
  };
  const originalLocation = runtimeGlobals.location;

  delete runtimeGlobals.__AGENT_THE_SPIRE_WS_BASES__;
  Object.assign(runtimeGlobals, {
    location: { host: "127.0.0.1:8080", href: "http://127.0.0.1:8080/" },
  });

  assert.throws(
    () => new BuildDeploySocket(),
    /workstation websocket endpoint is not configured/i,
  );

  if (typeof originalLocation === "undefined") {
    delete runtimeGlobals.location;
  } else {
    Object.assign(runtimeGlobals, { location: originalLocation });
  }
});
