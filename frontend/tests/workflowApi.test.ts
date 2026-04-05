import test from "node:test";
import assert from "node:assert/strict";

import {
  buildProject,
  createProject,
  generateModPlan,
  packageProject,
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

test("generateModPlan posts requirements and returns plan", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          mod_name: "DarkMage",
          summary: "summary",
          items: [],
        },
      });
    },
  });

  const plan = await generateModPlan("生成一个暗法师 mod");

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/plan");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(calls[0].init?.body, JSON.stringify({ requirements: "生成一个暗法师 mod" }));
  assert.equal(plan.mod_name, "DarkMage");
});

test("generateModPlan throws when backend returns 200 plus business error", async () => {
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async () =>
      createMockResponse({
        ok: true,
        body: {
          error: "requirements 不能为空",
        },
      }),
  });

  await assert.rejects(
    () => generateModPlan(""),
    /requirements 不能为空/,
  );
});

test("createProject posts name and target dir", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { project_path: "E:/STS2mod/MyMod" },
      });
    },
  });

  const result = await createProject({ name: "MyMod", target_dir: "E:/STS2mod" });

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/project/create");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(result.project_path, "E:/STS2mod/MyMod");
});

test("buildProject posts project root", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { success: true, output: "ok" },
      });
    },
  });

  const result = await buildProject({ project_root: "E:/STS2mod/MyMod" });

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/project/build");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(result.success, true);
});

test("packageProject posts project root", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { success: true },
      });
    },
  });

  const result = await packageProject({ project_root: "E:/STS2mod/MyMod" });

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/project/package");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(result.success, true);
});
