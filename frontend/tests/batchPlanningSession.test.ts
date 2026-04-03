import test from "node:test";
import assert from "node:assert/strict";

import {
  openBatchPlanningSocket,
  type BatchPlanningSocketSessionLike,
} from "../src/features/batch-generation/planningSession.ts";

class FakeBatchSocket implements BatchPlanningSocketSessionLike {
  waitOpenCalled = 0;
  sentMessages: object[] = [];
  closed = false;

  async waitOpen() {
    this.waitOpenCalled += 1;
    throw new Error("socket open failed");
  }

  send(data: object) {
    this.sentMessages.push(data);
  }

  close() {
    this.closed = true;
  }
}

test("openBatchPlanningSocket closes socket and reports error when websocket open fails", async () => {
  const socket = new FakeBatchSocket();
  const errors: string[] = [];

  const started = await openBatchPlanningSocket(socket, {
    requirements: "做一个暗法师 Mod",
    projectRoot: "E:/STS2mod",
    onOpenError(message) {
      errors.push(message);
    },
  });

  assert.equal(started, false);
  assert.equal(socket.waitOpenCalled, 1);
  assert.equal(socket.closed, true);
  assert.deepEqual(socket.sentMessages, []);
  assert.deepEqual(errors, ["socket open failed"]);
});
