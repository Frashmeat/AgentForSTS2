import test from "node:test";
import assert from "node:assert/strict";

import {
  getAdminExecution,
  listAdminAuditEvents,
  listAdminJobExecutions,
  listAdminQuotaRefunds,
} from "../src/shared/api/admin.ts";

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

test("admin job and execution getters compose resource paths", async () => {
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

  await listAdminJobExecutions(123);
  await getAdminExecution(9);

  assert.equal(calls[0].input, "/api/admin/jobs/123/executions");
  assert.equal(calls[1].input, "/api/admin/executions/9");
});

test("admin optional query params are only appended when present", async () => {
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

  await listAdminQuotaRefunds();
  await listAdminQuotaRefunds(7);
  await listAdminAuditEvents();
  await listAdminAuditEvents(123);

  assert.equal(calls[0].input, "/api/admin/quota/refunds");
  assert.equal(calls[1].input, "/api/admin/quota/refunds?user_id=7");
  assert.equal(calls[2].input, "/api/admin/audit/events");
  assert.equal(calls[3].input, "/api/admin/audit/events?job_id=123");
});
