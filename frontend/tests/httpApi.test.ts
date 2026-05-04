import test from "node:test";
import assert from "node:assert/strict";

import {
  buildApiPath,
  buildBackendUrl,
  requestBlob,
  requestJson,
  loadAppConfig,
  generateModPlan,
} from "../src/shared/api/index.ts";

interface MockResponseInit {
  ok: boolean;
  body?: unknown;
  text?: string;
  blob?: Blob;
  headers?: HeadersInit;
  status?: number;
  statusText?: string;
}

function createMockResponse(init: MockResponseInit) {
  return {
    ok: init.ok,
    status: init.status ?? (init.ok ? 200 : 500),
    statusText: init.statusText ?? "",
    headers: new Headers(init.headers),
    async json() {
      return init.body;
    },
    async text() {
      return init.text ?? JSON.stringify(init.body ?? {});
    },
    async blob() {
      return init.blob ?? new Blob([]);
    },
  };
}

test("requestJson sends json body with content type", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { ok: true },
      });
    },
  });

  const result = await requestJson<{ ok: boolean }>("/api/config", {
    method: "PATCH",
    body: { llm: { model: "gpt-5" } },
  });

  assert.deepEqual(result, { ok: true });
  assert.equal(calls[0].input, "/api/config");
  assert.equal(calls[0].init?.method, "PATCH");
  assert.equal((calls[0].init?.headers as Record<string, string>)["Content-Type"], "application/json");
  assert.equal(calls[0].init?.body, JSON.stringify({ llm: { model: "gpt-5" } }));
});

test("requestJson throws response text on non-ok response", async () => {
  Object.assign(globalThis, {
    fetch: async () =>
      createMockResponse({
        ok: false,
        text: '{"detail":"boom"}',
      }),
  });

  await assert.rejects(() => requestJson("/api/config"), /boom/);
});

test("requestJson unwraps json detail field on non-ok response", async () => {
  Object.assign(globalThis, {
    fetch: async () =>
      createMockResponse({
        ok: false,
        text: '{"detail":"authentication required"}',
      }),
  });

  await assert.rejects(() => requestJson("/api/me/profile", { backend: "web" }), /authentication required/);
});

test("requestJson unwraps structured error envelope message on non-ok response", async () => {
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      workstation: "http://127.0.0.1:7860",
    },
  });
  Object.assign(globalThis, {
    fetch: async () =>
      createMockResponse({
        ok: false,
        text: JSON.stringify({
          error: {
            code: "project_root_missing",
            message: "请先选择项目目录",
            detail: "default_project_root is empty",
          },
        }),
      }),
  });

  await assert.rejects(() => requestJson("/api/project/create", { backend: "workstation" }), /请先选择项目目录/);
});

test("requestBlob unwraps plain text response errors without json parse leakage", async () => {
  Object.assign(globalThis, {
    fetch: async () =>
      createMockResponse({
        ok: false,
        status: 400,
        text: "知识库包不完整",
      }),
  });

  try {
    await requestBlob("/api/knowledge/export-pack");
  } catch (error) {
    assert.match(error instanceof Error ? error.message : String(error), /知识库包不完整/);
    assert.doesNotMatch(error instanceof Error ? error.message : String(error), /JSON|SyntaxError/i);
    return;
  }
  assert.fail("requestBlob should reject non-ok plain text responses");
});

test("requestBlob wraps network errors with backend target and url", async () => {
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      web: "http://127.0.0.1:7870",
    },
    fetch: async () => {
      throw new TypeError("Failed to fetch");
    },
  });

  await assert.rejects(
    () => requestBlob("/api/platform/artifacts/1/download", { backend: "web" }),
    /无法连接Web 后端（http:\/\/127\.0\.0\.1:7870\/api\/platform\/artifacts\/1\/download）/,
  );
});

test("requestJson falls back to friendly message when non-ok response text is empty", async () => {
  Object.assign(globalThis, {
    fetch: async () =>
      createMockResponse({
        ok: false,
        text: "   ",
      }),
  });

  await assert.rejects(() => requestJson("/api/config"), /请求失败（HTTP 500）/);
});

test("requestJson routes to configured web backend when backend target is set", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      web: "http://127.0.0.1:7870",
    },
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { ok: true },
      });
    },
  });

  await requestJson<{ ok: boolean }>("/api/auth/me", { backend: "web" });

  assert.equal(calls[0].input, "http://127.0.0.1:7870/api/auth/me");
  assert.equal(calls[0].init?.credentials, "include");
});

test("buildBackendUrl falls back to current host on port 7870 for web backend", () => {
  const runtimeGlobals = globalThis as typeof globalThis & {
    __AGENT_THE_SPIRE_API_BASES__?: unknown;
    location?: Location | URL;
  };
  const originalLocation = runtimeGlobals.location;

  delete runtimeGlobals.__AGENT_THE_SPIRE_API_BASES__;
  Object.defineProperty(runtimeGlobals, "location", {
    value: new URL("http://127.0.0.1:7860/auth/login"),
    configurable: true,
  });

  try {
    assert.equal(buildBackendUrl("/api/auth/me", "web"), "http://127.0.0.1:7870/api/auth/me");
  } finally {
    if (typeof originalLocation === "undefined") {
      delete runtimeGlobals.location;
    } else {
      Object.defineProperty(runtimeGlobals, "location", {
        value: originalLocation,
        configurable: true,
      });
    }
  }
});

test("buildApiPath appends only defined query params", () => {
  assert.equal(
    buildApiPath("/api/platform/jobs/123/events", {
      user_id: 7,
      after_id: 9,
      limit: undefined,
    }),
    "/api/platform/jobs/123/events?user_id=7&after_id=9",
  );

  assert.equal(buildApiPath("/api/admin/quota/refunds", {}), "/api/admin/quota/refunds");
});

test("buildBackendUrl keeps same-origin by default and applies configured workstation base", () => {
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      workstation: "http://127.0.0.1:7860/",
    },
  });

  assert.equal(buildBackendUrl("/api/config", "same-origin"), "/api/config");
  assert.equal(buildBackendUrl("/api/config", "workstation"), "http://127.0.0.1:7860/api/config");
});

test("requestJson routes to configured workstation backend when backend target is set", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      workstation: "http://127.0.0.1:7860",
    },
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { ok: true },
      });
    },
  });

  await requestJson<{ ok: boolean }>("/api/config", { backend: "workstation" });

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/config");
  assert.equal(calls[0].init?.credentials, "include");
});

test("independent frontend without workstation endpoint fails loudly", async () => {
  const runtimeGlobals = globalThis as typeof globalThis & {
    __AGENT_THE_SPIRE_API_BASES__?: unknown;
    location?: Location | URL;
  };
  const originalLocation = runtimeGlobals.location;

  delete runtimeGlobals.__AGENT_THE_SPIRE_API_BASES__;
  Object.defineProperty(runtimeGlobals, "location", {
    value: new URL("http://127.0.0.1:8080/"),
    configurable: true,
  });

  try {
    await assert.rejects(() => loadAppConfig(), /当前前端没有配置 Workstation 后端地址/);
  } finally {
    if (typeof originalLocation === "undefined") {
      delete runtimeGlobals.location;
    } else {
      Object.defineProperty(runtimeGlobals, "location", {
        value: originalLocation,
        configurable: true,
      });
    }
  }
});

test("workflow api routes through workstation backend", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      workstation: "http://127.0.0.1:7860",
    },
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { plan: [] },
      });
    },
  });

  await generateModPlan("test requirements");

  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/plan");
});
