import test from "node:test";
import assert from "node:assert/strict";

import {
  getAdminExecution,
  listAdminAuditEvents,
  listAdminJobExecutions,
  listAdminQuotaRefunds,
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

test("admin endpoints expose typed execution and refund fields", async () => {
  let callIndex = 0;
  Object.assign(globalThis, {
    fetch: async () => {
      callIndex += 1;
      if (callIndex === 1) {
        return createMockResponse({
          ok: true,
          body: [
            {
              id: 1,
              job_id: 123,
              job_item_id: 1,
              status: "succeeded",
              provider: "openai",
              model: "gpt-5",
            },
          ],
        });
      }
      if (callIndex === 2) {
        return createMockResponse({
          ok: true,
          body: {
            id: 1,
            job_id: 123,
            job_item_id: 1,
            status: "succeeded",
            provider: "openai",
            model: "gpt-5",
            request_idempotency_key: "abc",
            input_summary: "创建卡牌代码",
            result_summary: "成功创建",
            error_summary: "",
            step_protocol_version: "v1",
            result_schema_version: "v1",
          },
        });
      }
      if (callIndex === 3) {
        return createMockResponse({
          ok: true,
          body: [
            {
              ai_execution_id: 1,
              charge_status: "refunded",
              refund_reason: "execution_failed",
            },
          ],
        });
      }
      return createMockResponse({
        ok: true,
        body: [
          {
            event_id: 1,
            event_type: "audit.logged",
          },
        ],
      });
    },
  });

  const executions = await listAdminJobExecutions(123);
  const execution = await getAdminExecution(1);
  const refunds = await listAdminQuotaRefunds();
  const auditEvents = await listAdminAuditEvents();

  assert.equal(executions[0].provider, "openai");
  assert.equal(execution.request_idempotency_key, "abc");
  assert.equal(refunds[0].refund_reason, "execution_failed");
  assert.equal(auditEvents[0].event_type, "audit.logged");
});
