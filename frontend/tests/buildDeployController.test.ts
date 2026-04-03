import test from "node:test";
import assert from "node:assert/strict";

import {
  createBuildDeployController,
  type BuildDeployControllerRuntime,
  type BuildDeploySocketLike,
} from "../src/components/buildDeployController.ts";
import type { BuildDeployState } from "../src/components/buildDeployModel.ts";

class FakeBuildDeploySocket implements BuildDeploySocketLike {
  private handlers = new Map<string, Array<(payload: any) => void>>();
  waitOpenCalled = 0;
  sentMessages: object[] = [];
  closed = false;

  on(event: "stream" | "done" | "error", handler: (payload: any) => void) {
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

  emit(event: "stream" | "done" | "error", payload: any) {
    const list = this.handlers.get(event) ?? [];
    list.forEach((handler) => handler(payload));
  }
}

function createRuntime() {
  let state: BuildDeployState = {
    stage: "idle",
    action: null,
    log: [],
    deployedTo: null,
    summary: null,
    errorMsg: null,
  };
  let socket: BuildDeploySocketLike | null = null;
  const history: BuildDeployState[] = [state];

  const runtime: BuildDeployControllerRuntime = {
    closeSocket() {
      socket?.close();
    },
    setSocket(nextSocket) {
      socket = nextSocket;
    },
    setState(nextState) {
      state = typeof nextState === "function" ? nextState(state) : nextState;
      history.push(state);
    },
  };

  return {
    runtime,
    getState: () => state,
    getSocket: () => socket,
    history,
  };
}

test("createBuildDeployController runs build action through shared state flow", async () => {
  const runtime = createRuntime();
  const controller = createBuildDeployController(runtime.runtime, {
    buildProject: async () => ({
      success: true,
      output: "Build started\nBuild succeeded",
    }),
    packageProject: async () => ({ success: true }),
    createSocket: () => new FakeBuildDeploySocket(),
  });

  await controller.run("build", " E:/STS2mod/MyMod ");

  assert.deepEqual(runtime.getState(), {
    stage: "done",
    action: "build",
    log: ["Build started", "Build succeeded"],
    deployedTo: null,
    summary: "构建成功",
    errorMsg: null,
  });
  assert.equal(runtime.history[1]?.stage, "running");
});

test("createBuildDeployController wires deploy socket events into shared state", async () => {
  const runtime = createRuntime();
  const socket = new FakeBuildDeploySocket();
  const controller = createBuildDeployController(runtime.runtime, {
    buildProject: async () => ({
      success: true,
      output: "",
    }),
    packageProject: async () => ({ success: true }),
    createSocket: () => socket,
  });

  await controller.run("deploy", " E:/STS2mod/MyMod ");
  socket.emit("stream", { chunk: "building..." });
  socket.emit("done", { deployed_to: "E:/Mods/MyMod" });

  assert.equal(socket.waitOpenCalled, 1);
  assert.deepEqual(socket.sentMessages, [{ project_root: "E:/STS2mod/MyMod" }]);
  assert.deepEqual(runtime.getState(), {
    stage: "done",
    action: "deploy",
    log: ["building..."],
    deployedTo: "E:/Mods/MyMod",
    summary: "已部署",
    errorMsg: null,
  });
});

test("createBuildDeployController reset closes socket and clears state", () => {
  const runtime = createRuntime();
  const socket = new FakeBuildDeploySocket();
  const controller = createBuildDeployController(runtime.runtime, {
    buildProject: async () => ({
      success: true,
      output: "",
    }),
    packageProject: async () => ({ success: true }),
    createSocket: () => socket,
  });

  runtime.runtime.setSocket(socket);
  runtime.runtime.setState({
    stage: "error",
    action: "deploy",
    log: ["boom"],
    deployedTo: null,
    summary: null,
    errorMsg: "boom",
  });

  controller.reset();

  assert.equal(socket.closed, true);
  assert.deepEqual(runtime.getState(), {
    stage: "idle",
    action: null,
    log: [],
    deployedTo: null,
    summary: null,
    errorMsg: null,
  });
});

test("createBuildDeployController surfaces build request failure as error state", async () => {
  const runtime = createRuntime();
  const controller = createBuildDeployController(runtime.runtime, {
    buildProject: async () => {
      throw new Error("build exploded");
    },
    packageProject: async () => ({ success: true }),
    createSocket: () => new FakeBuildDeploySocket(),
  });

  await controller.run("build", "E:/STS2mod/MyMod");

  assert.deepEqual(runtime.getState(), {
    stage: "error",
    action: "build",
    log: [],
    deployedTo: null,
    summary: null,
    errorMsg: "build exploded",
  });
});

test("createBuildDeployController surfaces deploy socket open failure as error state", async () => {
  const runtime = createRuntime();
  const socket = new FakeBuildDeploySocket();
  socket.waitOpen = async () => {
    throw new Error("socket open failed");
  };
  const controller = createBuildDeployController(runtime.runtime, {
    buildProject: async () => ({
      success: true,
      output: "",
    }),
    packageProject: async () => ({ success: true }),
    createSocket: () => socket,
  });

  await controller.run("deploy", "E:/STS2mod/MyMod");

  assert.equal(runtime.getSocket(), null);
  assert.deepEqual(runtime.getState(), {
    stage: "error",
    action: "deploy",
    log: [],
    deployedTo: null,
    summary: null,
    errorMsg: "socket open failed",
  });
});

test("createBuildDeployController preserves streamed log when deploy emits error event", async () => {
  const runtime = createRuntime();
  const socket = new FakeBuildDeploySocket();
  const controller = createBuildDeployController(runtime.runtime, {
    buildProject: async () => ({
      success: true,
      output: "",
    }),
    packageProject: async () => ({ success: true }),
    createSocket: () => socket,
  });

  await controller.run("deploy", "E:/STS2mod/MyMod");
  socket.emit("stream", { chunk: "building..." });
  socket.emit("error", { message: "deploy failed" });

  assert.deepEqual(runtime.getState(), {
    stage: "error",
    action: "deploy",
    log: ["building..."],
    deployedTo: null,
    summary: null,
    errorMsg: "deploy failed",
  });
});
