import test from "node:test";
import assert from "node:assert/strict";

import {
  createBatchPlanningController,
  type BatchPlanningRuntime,
  type BatchPlanningSocketLike,
} from "../src/features/batch-generation/planningController.ts";

class FakeBatchSocket implements BatchPlanningSocketLike {
  waitOpenCalled = 0;
  sentMessages: object[] = [];
  closed = false;

  async waitOpen() {
    this.waitOpenCalled += 1;
  }

  send(data: object) {
    this.sentMessages.push(data);
  }

  close() {
    this.closed = true;
  }
}

function createRuntime() {
  let socket: BatchPlanningSocketLike | null = null;
  const events: string[] = [];

  const runtime: BatchPlanningRuntime = {
    closeSocket() {
      socket?.close();
      events.push("closeSocket");
    },
    setSocket(nextSocket) {
      socket = nextSocket;
      events.push(`setSocket:${nextSocket ? "socket" : "null"}`);
    },
    clearProjectCreationFeedback() {
      events.push("clearProjectCreationFeedback");
    },
    setRestoredSnapshotMode(value) {
      events.push(`setRestoredSnapshotMode:${value}`);
    },
    setRestoredApprovalRefreshPending(value) {
      events.push(`setRestoredApprovalRefreshPending:${value}`);
    },
    dispatchPlanningStarted() {
      events.push("dispatchPlanningStarted");
    },
    clearPlan() {
      events.push("clearPlan");
    },
    applyGeneratedPlan(plan) {
      events.push(`applyGeneratedPlan:${plan.mod_name}`);
    },
    registerSocketHandlers() {
      events.push("registerSocketHandlers");
    },
    reportWorkflowError(message) {
      events.push(`reportWorkflowError:${message}`);
    },
  };

  return {
    runtime,
    events,
    getSocket: () => socket,
  };
}

test("batch planning controller prepares state and starts websocket planning", async () => {
  const fakeSocket = new FakeBatchSocket();
  const runtime = createRuntime();
  const controller = createBatchPlanningController(runtime.runtime, {
    createSocket: () => fakeSocket,
    generateModPlan: async () => ({
      mod_name: "Unused",
      summary: "",
      items: [],
    }),
  });

  await controller.startSocketPlanning("做一个暗法师 Mod", "E:/STS2mod");

  assert.deepEqual(runtime.events, [
    "closeSocket",
    "setSocket:null",
    "clearProjectCreationFeedback",
    "setRestoredSnapshotMode:false",
    "setRestoredApprovalRefreshPending:false",
    "dispatchPlanningStarted",
    "clearPlan",
    "setSocket:socket",
    "registerSocketHandlers",
  ]);
  assert.equal(fakeSocket.waitOpenCalled, 1);
  assert.deepEqual(fakeSocket.sentMessages, [
    { action: "start", requirements: "做一个暗法师 Mod", project_root: "E:/STS2mod" },
  ]);
});

test("batch planning controller prepares state and starts HTTP fallback planning", async () => {
  const runtime = createRuntime();
  const controller = createBatchPlanningController(runtime.runtime, {
    createSocket: () => new FakeBatchSocket(),
    generateModPlan: async (requirements) => ({
      mod_name: requirements,
      summary: "summary",
      items: [],
    }),
  });

  await controller.startHttpPlanning("DarkMage", "E:/STS2mod");

  assert.deepEqual(runtime.events, [
    "closeSocket",
    "setSocket:null",
    "clearProjectCreationFeedback",
    "setRestoredSnapshotMode:false",
    "setRestoredApprovalRefreshPending:false",
    "dispatchPlanningStarted",
    "clearPlan",
    "applyGeneratedPlan:DarkMage",
  ]);
});

test("batch planning controller reports socket open failure through shared error path", async () => {
  const fakeSocket = new FakeBatchSocket();
  fakeSocket.waitOpen = async () => {
    throw new Error("socket open failed");
  };
  const runtime = createRuntime();
  const controller = createBatchPlanningController(runtime.runtime, {
    createSocket: () => fakeSocket,
    generateModPlan: async () => ({
      mod_name: "Unused",
      summary: "",
      items: [],
    }),
  });

  await controller.startSocketPlanning("做一个暗法师 Mod", "E:/STS2mod");

  assert.match(runtime.events[runtime.events.length - 1] ?? "", /reportWorkflowError:socket open failed/);
});

test("batch planning controller reports HTTP fallback error through shared error path", async () => {
  const runtime = createRuntime();
  const controller = createBatchPlanningController(runtime.runtime, {
    createSocket: () => new FakeBatchSocket(),
    generateModPlan: async () => {
      throw new Error("http fallback failed");
    },
  });

  await controller.startHttpPlanning("DarkMage", "E:/STS2mod");

  assert.match(runtime.events[runtime.events.length - 1] ?? "", /reportWorkflowError:http fallback failed/);
});
