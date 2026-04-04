import test from "node:test";
import assert from "node:assert/strict";

import {
  detectAppPaths,
  loadLocalAiCapabilityStatus,
  loadAppConfig,
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

test("loadAppConfig reads /api/config", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
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

  assert.equal(calls[0].input, "/api/config");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(config.default_project_root, "E:/STS2mod");
});

test("detectAppPaths reads detect paths endpoint", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
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

  assert.equal(calls[0].input, "/api/config/detect_paths");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(result.notes[0], "ok");
});

test("updateAppConfig patches config json body", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
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

  assert.equal(calls[0].input, "/api/config");
  assert.equal(calls[0].init?.method, "PATCH");
  assert.equal(calls[0].init?.body, JSON.stringify({ default_project_root: "E:/STS2mod/new" }));
  assert.equal(result.default_project_root, "E:/STS2mod/new");
});

test("loadLocalAiCapabilityStatus reads boolean-only capability endpoint", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          text_ai_available: true,
          image_ai_available: false,
        },
      });
    },
  });

  const result = await loadLocalAiCapabilityStatus();

  assert.equal(calls[0].input, "/api/config/local_ai_capability_status");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(result.text_ai_available, true);
  assert.equal(result.image_ai_available, false);
});
