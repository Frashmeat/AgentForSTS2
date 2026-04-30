import test from "node:test";
import assert from "node:assert/strict";

import {
  createModEditorAnalysisController,
  createModEditorModifyController,
  type ModEditorAnalysisRuntime,
  type ModEditorAnalysisSocketLike,
  type ModEditorModifyRuntime,
  type ModEditorModifySocketLike,
} from "../src/features/mod-editor/controller.ts";

class FakeModAnalysisSocket implements ModEditorAnalysisSocketLike {
  private handlers = new Map<string, Array<(payload: any) => void>>();
  waitOpenCalled = 0;
  sentMessages: object[] = [];
  closed = false;

  on(event: "stage_update" | "scan_info" | "stream" | "done" | "error", handler: (payload: any) => void) {
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

  emit(event: "stage_update" | "scan_info" | "stream" | "done" | "error", payload: any) {
    const list = this.handlers.get(event) ?? [];
    list.forEach((handler) => handler(payload));
  }
}

class FakeWorkflowSocket implements ModEditorModifySocketLike {
  private handlers = new Map<string, Array<(payload: any) => void>>();
  waitOpenCalled = 0;
  sentMessages: object[] = [];
  closed = false;

  on(event: "stage_update" | "progress" | "agent_stream" | "done" | "error", handler: (payload: any) => void) {
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

  emit(event: "stage_update" | "progress" | "agent_stream" | "done" | "error", payload: any) {
    const list = this.handlers.get(event) ?? [];
    list.forEach((handler) => handler(payload));
  }
}

function createAnalysisRuntime() {
  let socket: ModEditorAnalysisSocketLike | null = null;
  const events: string[] = [];

  const runtime: ModEditorAnalysisRuntime = {
    closeAnalysisSocket() {
      socket?.close();
      events.push("closeAnalysisSocket");
    },
    setAnalysisSocket(nextSocket) {
      socket = nextSocket;
      events.push(`setAnalysisSocket:${nextSocket ? "socket" : "null"}`);
    },
    clearProjectCreationFeedback() {
      events.push("clearProjectCreationFeedback");
    },
    startAnalysis() {
      events.push("startAnalysis");
    },
    applyAnalysisStageMessage(message) {
      events.push(`applyAnalysisStageMessage:${message}`);
    },
    applyAnalysisScanInfo(files) {
      events.push(`applyAnalysisScanInfo:${files}`);
    },
    appendAnalysisChunk(chunk) {
      events.push(`appendAnalysisChunk:${chunk}`);
    },
    completeAnalysis() {
      events.push("completeAnalysis");
    },
    failAnalysis(message) {
      events.push(`failAnalysis:${message}`);
    },
    resetAnalysis() {
      events.push("resetAnalysis");
    },
  };

  return {
    runtime,
    events,
    getSocket: () => socket,
  };
}

function createModifyRuntime() {
  let socket: ModEditorModifySocketLike | null = null;
  const events: string[] = [];

  const runtime: ModEditorModifyRuntime = {
    closeModifySocket() {
      socket?.close();
      events.push("closeModifySocket");
    },
    setModifySocket(nextSocket) {
      socket = nextSocket;
      events.push(`setModifySocket:${nextSocket ? "socket" : "null"}`);
    },
    clearProjectCreationFeedback() {
      events.push("clearProjectCreationFeedback");
    },
    startModify() {
      events.push("startModify");
    },
    applyModifyStageMessage(message) {
      events.push(`applyModifyStageMessage:${message}`);
    },
    appendModifyLog(line) {
      events.push(`appendModifyLog:${line}`);
    },
    completeModify(success) {
      events.push(`completeModify:${success}`);
    },
    failModify(message) {
      events.push(`failModify:${message}`);
    },
    resetModify() {
      events.push("resetModify");
    },
  };

  return {
    runtime,
    events,
    getSocket: () => socket,
  };
}

test("mod editor analysis controller prepares state and starts socket run", async () => {
  const socket = new FakeModAnalysisSocket();
  const runtime = createAnalysisRuntime();
  const controller = createModEditorAnalysisController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.run(" E:/STS2mod/MyMod ");
  socket.emit("stage_update", { message: "正在扫描" });
  socket.emit("scan_info", { files: 12 });
  socket.emit("stream", { chunk: "分析输出" });
  socket.emit("done", {});

  assert.deepEqual(runtime.events, [
    "clearProjectCreationFeedback",
    "closeAnalysisSocket",
    "setAnalysisSocket:null",
    "startAnalysis",
    "setAnalysisSocket:socket",
    "applyAnalysisStageMessage:正在扫描",
    "applyAnalysisScanInfo:12",
    "appendAnalysisChunk:分析输出",
    "completeAnalysis",
  ]);
  assert.equal(socket.waitOpenCalled, 1);
  assert.deepEqual(socket.sentMessages, [{ project_root: "E:/STS2mod/MyMod" }]);
});

test("mod editor analysis controller reports socket open failure through shared error path", async () => {
  const socket = new FakeModAnalysisSocket();
  socket.waitOpen = async () => {
    throw new Error("analysis socket failed");
  };
  const runtime = createAnalysisRuntime();
  const controller = createModEditorAnalysisController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.run("E:/STS2mod/MyMod");

  assert.equal(runtime.getSocket(), null);
  assert.match(runtime.events[runtime.events.length - 1] ?? "", /failAnalysis:analysis socket failed/);
});

test("mod editor modify controller prepares state and starts workflow run", async () => {
  const socket = new FakeWorkflowSocket();
  const runtime = createModifyRuntime();
  const controller = createModEditorModifyController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.run(" E:/STS2mod/MyMod ", " 调整 DarkBlade 伤害 ", "分析结论");
  socket.emit("stage_update", { message: "正在修改" });
  socket.emit("progress", { message: "定位文件" });
  socket.emit("agent_stream", { chunk: "修改代码" });
  socket.emit("done", { success: true });

  assert.deepEqual(runtime.events, [
    "clearProjectCreationFeedback",
    "closeModifySocket",
    "setModifySocket:null",
    "startModify",
    "setModifySocket:socket",
    "applyModifyStageMessage:正在修改",
    "appendModifyLog:定位文件",
    "appendModifyLog:修改代码",
    "completeModify:true",
  ]);
  assert.equal(socket.waitOpenCalled, 1);
  assert.deepEqual(socket.sentMessages, [
    {
      action: "start",
      asset_type: "custom_code",
      asset_name: "ModModification",
      description: "调整 DarkBlade 伤害",
      project_root: "E:/STS2mod/MyMod",
      implementation_notes:
        "当前 mod 分析概况：\n分析结论\n\n这是对已有 mod 的修改请求，请定位到相关文件进行修改，不要新建不必要的文件。",
    },
  ]);
});

test("mod editor modify controller reports socket open failure through shared error path", async () => {
  const socket = new FakeWorkflowSocket();
  socket.waitOpen = async () => {
    throw new Error("modify socket failed");
  };
  const runtime = createModifyRuntime();
  const controller = createModEditorModifyController(runtime.runtime, {
    createSocket: () => socket,
  });

  await controller.run("E:/STS2mod/MyMod", "调整", "");

  assert.equal(runtime.getSocket(), null);
  assert.match(runtime.events[runtime.events.length - 1] ?? "", /failModify:modify socket failed/);
});

test("mod editor controllers reset sockets and state through runtime helpers", () => {
  const analysisSocket = new FakeModAnalysisSocket();
  const analysisRuntime = createAnalysisRuntime();
  analysisRuntime.runtime.setAnalysisSocket(analysisSocket);
  const analysisController = createModEditorAnalysisController(analysisRuntime.runtime, {
    createSocket: () => analysisSocket,
  });

  const modifySocket = new FakeWorkflowSocket();
  const modifyRuntime = createModifyRuntime();
  modifyRuntime.runtime.setModifySocket(modifySocket);
  const modifyController = createModEditorModifyController(modifyRuntime.runtime, {
    createSocket: () => modifySocket,
  });

  analysisController.reset();
  modifyController.reset();

  assert.equal(analysisSocket.closed, true);
  assert.equal(modifySocket.closed, true);
  assert.deepEqual(analysisRuntime.events.slice(-3), [
    "closeAnalysisSocket",
    "setAnalysisSocket:null",
    "resetAnalysis",
  ]);
  assert.deepEqual(modifyRuntime.events.slice(-3), ["closeModifySocket", "setModifySocket:null", "resetModify"]);
});
