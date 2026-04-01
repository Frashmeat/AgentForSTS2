import test from "node:test";
import assert from "node:assert/strict";

import { buildApiPath, requestJson } from "../src/shared/api/http.ts";

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
        text: "{\"detail\":\"boom\"}",
      }),
  });

  await assert.rejects(
    () => requestJson("/api/config"),
    /boom/,
  );
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

  assert.equal(
    buildApiPath("/api/admin/quota/refunds", {}),
    "/api/admin/quota/refunds",
  );
});
