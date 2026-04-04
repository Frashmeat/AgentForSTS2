import test from "node:test";
import assert from "node:assert/strict";

import {
  getMyJob,
  getMyProfile,
  getMyQuota,
  listMyJobItems,
  listMyJobs,
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

test("me api defaults to current user routes without user_id", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: [],
      });
    },
  });

  await getMyProfile();
  await getMyQuota();
  await listMyJobs();
  await getMyJob(123);
  await listMyJobItems(123);

  assert.equal(calls[0].input, "/api/me/profile");
  assert.equal(calls[1].input, "/api/me/quota");
  assert.equal(calls[2].input, "/api/me/jobs");
  assert.equal(calls[3].input, "/api/me/jobs/123");
  assert.equal(calls[4].input, "/api/me/jobs/123/items");
});

test("me api defaults to web backend when runtime base is configured", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      web: "http://127.0.0.1:7870",
    },
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          user_id: 7,
          username: "luna",
          email: "luna@example.com",
          email_verified: true,
          created_at: "2026-04-03T10:00:00+00:00",
        },
      });
    },
  });

  await getMyProfile();

  assert.equal(calls[0].input, "http://127.0.0.1:7870/api/me/profile");
});
