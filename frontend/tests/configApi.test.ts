import test from "node:test";
import assert from "node:assert/strict";

import {
  cancelDetectAppPathsTask,
  detectAppPaths,
  getDetectAppPathsTask,
  loadLocalAiCapabilityStatus,
  loadAppConfig,
  loadPlatformQueueWorkerStatus,
  pickAppPath,
  startDetectAppPaths,
  updateAppConfig,
} from "../src/shared/api/index.ts";

interface MockResponseInit {
  ok: boolean;
  body?: unknown;
  text?: string;
}

function createMockResponse(init: MockResponseInit) {
  return {
    ok: init.ok,
    async json() {
      return init.body;
    },
    async text() {
      return init.text ?? JSON.stringify(init.body ?? {});
    },
  };
}

function setWorkstationApiBase() {
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      workstation: "http://127.0.0.1:7860",
    },
  });
}

test("loadAppConfig reads /api/config", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { default_project_root: "E:/STS2mod" },
      });
    },
  });

  const config = await loadAppConfig();

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(config.default_project_root, "E:/STS2mod");
});

test("detectAppPaths reads detect paths endpoint", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          sts2_path: "E:/steam/steamapps/common/Slay the Spire 2",
          godot_exe_path: "C:/tools/Godot.exe",
          notes: ["ok"],
        },
      });
    },
  });

  const result = await detectAppPaths();

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config/detect_paths");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(result.notes[0], "ok");
});

test("startDetectAppPaths posts detect task request", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          task_id: "task-1",
          status: "running",
          current_step: "开始检测",
          notes: ["开始检测"],
          can_cancel: true,
        },
      });
    },
  });

  const result = await startDetectAppPaths();

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config/detect_paths/start");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(result.task_id, "task-1");
});

test("getDetectAppPathsTask reads detect task status endpoint", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          task_id: "task-1",
          status: "completed",
          current_step: "已完成",
          notes: ["完成"],
          sts2_path: "E:/steam/steamapps/common/Slay the Spire 2",
          can_cancel: false,
        },
      });
    },
  });

  const result = await getDetectAppPathsTask("task-1");

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config/detect_paths/task-1");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(result.status, "completed");
});

test("cancelDetectAppPathsTask posts cancel request", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          task_id: "task-1",
          status: "cancelled",
          current_step: "已取消",
          notes: ["已取消"],
          can_cancel: false,
        },
      });
    },
  });

  const result = await cancelDetectAppPathsTask("task-1");

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config/detect_paths/task-1/cancel");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(result.status, "cancelled");
});

test("pickAppPath posts pick path request to workstation config endpoint", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          path: "C:/tools/Godot_v4.5.1-stable_mono_win64/Godot_v4.5.1-stable_mono_win64.exe",
        },
      });
    },
  });

  const result = await pickAppPath({
    kind: "file",
    title: "选择 Godot 4.5.1 Mono 可执行文件",
    initial_path: "C:/tools",
    filters: [["Godot executable", "*.exe"]],
  });

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config/pick_path");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(
    calls[0].init?.body,
    JSON.stringify({
      kind: "file",
      title: "选择 Godot 4.5.1 Mono 可执行文件",
      initial_path: "C:/tools",
      filters: [["Godot executable", "*.exe"]],
    }),
  );
  assert.equal(result.path, "C:/tools/Godot_v4.5.1-stable_mono_win64/Godot_v4.5.1-stable_mono_win64.exe");
});

test("updateAppConfig patches config json body", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { default_project_root: "E:/STS2mod/new" },
      });
    },
  });

  const result = await updateAppConfig({
    default_project_root: "E:/STS2mod/new",
  });

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config");
  assert.equal(calls[0].init?.method, "PATCH");
  assert.equal(calls[0].init?.body, JSON.stringify({ default_project_root: "E:/STS2mod/new" }));
  assert.equal(result.default_project_root, "E:/STS2mod/new");
});

test("loadLocalAiCapabilityStatus reads capability endpoint with missing reasons", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          text_ai_available: true,
          code_agent_available: true,
          image_ai_available: false,
          text_ai_missing_reasons: [],
          code_agent_missing_reasons: [],
          image_ai_missing_reasons: ["请先在设置中填写图像 API Key。"],
        },
      });
    },
  });

  const result = await loadLocalAiCapabilityStatus();

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config/local_ai_capability_status");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(result.text_ai_available, true);
  assert.equal(result.code_agent_available, true);
  assert.equal(result.image_ai_available, false);
  assert.deepEqual(result.image_ai_missing_reasons, ["请先在设置中填写图像 API Key。"]);
});

test("loadPlatformQueueWorkerStatus reads queue worker status endpoint", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          available: true,
          owner_id: "queue-worker:123",
          owner_scope: "system_queue_worker",
          is_leader: true,
          leader_epoch: 3,
          failover_window_seconds: 10,
          recent_leader_events: [],
        },
      });
    },
  });

  const result = await loadPlatformQueueWorkerStatus();

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config/platform_queue_worker_status");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(result.available, true);
  assert.equal(result.leader_epoch, 3);
});
