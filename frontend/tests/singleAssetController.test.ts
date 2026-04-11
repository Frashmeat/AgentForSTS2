import test from "node:test";
import assert from "node:assert/strict";

import {
  createSingleAssetWorkflowController,
  type SingleAssetWorkflowRuntime,
  type SingleAssetWorkflowSocketLike,
} from "../src/features/single-asset/controller.ts";

class FakeSingleAssetSocket implements SingleAssetWorkflowSocketLike {
  private handlers = new Map<string, Array<(payload: any) => void>>();
  waitOpenCalled = 0;
  sentMessages: object[] = [];
  closed = false;

  on(event: string, handler: (payload: any) => void) {
    const list = this.handlers.get(event) ?? [];
    this.handlers.set(event, [...list, handler]);
    return this;
  }

  async waitOpen() {
    this.waitOpenCalled += 1;
  }

  send(data: object) {
    this.sentMessages.push(data);
  }

  close() {
    this.closed = true;
  }

  emit(event: string, payload: any) {
    const list = this.handlers.get(event) ?? [];
    list.forEach((handler) => handler(payload));
  }
}

function createRuntime() {
  let socket: SingleAssetWorkflowSocketLike | null = null;
  const events: string[] = [];

  const runtime: SingleAssetWorkflowRuntime = {
    closeSocket() {
      socket?.close();
      events.push("closeSocket");
    },
    setSocket(nextSocket) {
      socket = nextSocket;
      events.push(`setSocket:${nextSocket ? "socket" : "null"}`);
    },
    getSocket() {
      return socket;
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
    dispatchWorkflow(action) {
      const summary = Object.entries(action)
        .map(([key, value]) => `${key}=${String(value)}`)
        .join(",");
      events.push(`dispatch:${summary}`);
    },
    reportWorkflowError(message, traceback) {
      events.push(`reportWorkflowError:${message}:${traceback ?? "null"}`);
    },
  };

  return {
    runtime,
    events,
    getSocket: () => socket,
  };
}

test("single asset controller starts workflow and sends start payload", async () => {
  const socket = new FakeSingleAssetSocket();
  const runtime = createRuntime();
  const controller = createSingleAssetWorkflowController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.start({
    assetType: "relic",
    assetName: "DarkRelic",
    description: "desc",
    projectRoot: " E:/STS2mod/MyMod ",
    imageMode: "ai",
    uploadedImageB64: "",
    uploadedImageName: "",
    autoMode: false,
  });

  assert.deepEqual(runtime.events.slice(0, 6), [
    "clearProjectCreationFeedback",
    "setRestoredSnapshotMode:false",
    "setRestoredApprovalRefreshPending:false",
    "closeSocket",
    "setSocket:null",
    "dispatch:type=workflow_started,imageMode=ai",
  ]);
  assert.equal(socket.waitOpenCalled, 1);
  assert.deepEqual(socket.sentMessages, [
    {
      action: "start",
      asset_type: "relic",
      asset_name: "DarkRelic",
      description: "desc",
      project_root: "E:/STS2mod/MyMod",
    },
  ]);
});

test("single asset controller auto mode confirms prompt and selects first image", async () => {
  const socket = new FakeSingleAssetSocket();
  const runtime = createRuntime();
  const controller = createSingleAssetWorkflowController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.start({
    assetType: "relic",
    assetName: "DarkRelic",
    description: "desc",
    projectRoot: "E:/STS2mod/MyMod",
    imageMode: "ai",
    uploadedImageB64: "",
    uploadedImageName: "",
    autoMode: true,
  });

  socket.emit("prompt_preview", {
    prompt: "draw relic",
    negative_prompt: "blur",
  });
  socket.emit("image_ready", {
    index: 0,
    image: "img-b64",
    prompt: "draw relic",
  });

  assert.deepEqual(socket.sentMessages.slice(1), [
    { action: "confirm", prompt: "draw relic", negative_prompt: "blur" },
    { action: "select", index: 0 },
  ]);
  assert.match(runtime.events.join("\n"), /dispatch:type=gen_log_appended,message=自动模式：跳过 prompt 确认/);
  assert.match(runtime.events.join("\n"), /dispatch:type=gen_log_appended,message=自动模式：自动选第 1 张图/);
  assert.match(runtime.events.join("\n"), /dispatch:type=stage_changed,stage=agent_running/);
});

test("single asset controller sends upload payload and moves to agent stage immediately", async () => {
  const socket = new FakeSingleAssetSocket();
  const runtime = createRuntime();
  const controller = createSingleAssetWorkflowController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.start({
    assetType: "relic",
    assetName: "DarkRelic",
    description: "desc",
    projectRoot: "E:/STS2mod/MyMod",
    imageMode: "upload",
    uploadedImageB64: "upload-b64",
    uploadedImageName: "test.png",
    autoMode: false,
  });

  assert.match(runtime.events.join("\n"), /dispatch:type=stage_changed,stage=agent_running/);
  assert.deepEqual(socket.sentMessages, [
    {
      action: "start",
      asset_type: "relic",
      asset_name: "DarkRelic",
      description: "desc",
      project_root: "E:/STS2mod/MyMod",
      provided_image_b64: "upload-b64",
      provided_image_name: "test.png",
    },
  ]);
});

test("single asset controller forwards commands through active socket and resets cleanly", async () => {
  const socket = new FakeSingleAssetSocket();
  const runtime = createRuntime();
  const controller = createSingleAssetWorkflowController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.start({
    assetType: "relic",
    assetName: "DarkRelic",
    description: "desc",
    projectRoot: "E:/STS2mod/MyMod",
    imageMode: "ai",
    uploadedImageB64: "",
    uploadedImageName: "",
    autoMode: false,
  });

  controller.confirmPrompt("prompt", "negative");
  controller.selectImage(2);
  controller.generateMore("new prompt", "negative", 3);
  controller.proceedApproval();
  controller.reset();

  assert.deepEqual(socket.sentMessages.slice(1), [
    { action: "confirm", prompt: "prompt", negative_prompt: "negative" },
    { action: "select", index: 2 },
    { action: "generate_more", prompt: "new prompt", negative_prompt: "negative" },
    { action: "approve_all" },
  ]);
  assert.equal(socket.closed, true);
  assert.deepEqual(runtime.events.slice(-4), [
    "dispatch:type=generate_more_requested,batchOffset=3",
    "closeSocket",
    "setSocket:null",
    "dispatch:type=workflow_reset",
  ]);
});
