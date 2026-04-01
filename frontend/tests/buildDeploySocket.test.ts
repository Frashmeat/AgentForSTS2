import test from "node:test";
import assert from "node:assert/strict";

import { BuildDeploySocket } from "../src/lib/build_deploy_ws.ts";

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

test("BuildDeploySocket connects to build-deploy websocket endpoint", () => {
  new BuildDeploySocket();

  assert.equal(MockWebSocket.instances[0]?.url, "ws://localhost:7860/api/ws/build-deploy");
});

test("BuildDeploySocket dispatches build-deploy events", () => {
  const socket = new BuildDeploySocket();
  const received: Array<Record<string, unknown>> = [];

  socket.on("stream", (data) => {
    received.push(data as Record<string, unknown>);
  });
  socket.on("done", (data) => {
    received.push(data as Record<string, unknown>);
  });

  MockWebSocket.instances[0].emitMessage({ event: "stream", chunk: "building..." });
  MockWebSocket.instances[0].emitMessage({ event: "done", success: true, deployed_to: "E:/Mods/MyMod", files: ["MyMod.dll"] });

  assert.deepEqual(received, [
    { event: "stream", stage: "stream", chunk: "building..." },
    { event: "done", stage: "done", success: true, deployed_to: "E:/Mods/MyMod", files: ["MyMod.dll"] },
  ]);
});
