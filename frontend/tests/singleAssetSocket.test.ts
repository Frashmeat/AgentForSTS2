import test from "node:test";
import assert from "node:assert/strict";

import { createSingleAssetSocket } from "../src/lib/single_asset_ws.ts";

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
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
}

test.beforeEach(() => {
  MockWebSocket.instances = [];
  Object.assign(globalThis, {
    WebSocket: MockWebSocket,
    location: { host: "localhost:7860" },
  });
});

test("createSingleAssetSocket uses create websocket endpoint under unified flag", () => {
  createSingleAssetSocket({
    use_modular_single_workflow: false,
    use_modular_batch_workflow: false,
    use_unified_ws_contract: true,
  });

  assert.equal(MockWebSocket.instances[0]?.url, "ws://localhost:7860/api/ws/create");
});

test("createSingleAssetSocket reports unexpected close as error event", async () => {
  const socket = createSingleAssetSocket({
    use_modular_single_workflow: false,
    use_modular_batch_workflow: false,
    use_unified_ws_contract: true,
  });
  let message = "";

  socket.on("error", (data) => {
    message = data.message;
  });

  const pending = socket.waitOpen();
  MockWebSocket.instances[0].emitOpen();
  await pending;

  MockWebSocket.instances[0].emitClose({ wasClean: false, code: 1006 });

  assert.match(message, /WebSocket 连接意外断开/);
});
