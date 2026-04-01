import test from "node:test";
import assert from "node:assert/strict";

import {
  cancelPlatformJob,
  createPlatformJob,
  getPlatformJob,
  getPlatformQuota,
  listPlatformJobEvents,
  listPlatformJobItems,
  listPlatformJobs,
  startPlatformJob,
} from "../src/shared/api/platform.ts";

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

test("createPlatformJob posts body with user_id query", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { id: 123, status: "pending" },
      });
    },
  });

  const result = await createPlatformJob(7, {
    job_type: "batch_mod_generation",
    workflow_version: "v1",
    input_summary: "创建暗法师 Mod",
    created_from: "platform_api",
    items: [],
  });

  assert.equal(calls[0].input, "/api/platform/jobs?user_id=7");
  assert.equal(calls[0].init?.method, "POST");
  assert.equal(result.id, 123);
});

test("startPlatformJob and cancelPlatformJob include job id and user id", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { ok: true, status: "running" },
      });
    },
  });

  await startPlatformJob(7, 123, { triggered_by: "user" });
  await cancelPlatformJob(7, 123, { reason: "取消" });

  assert.equal(calls[0].input, "/api/platform/jobs/123/start?user_id=7");
  assert.equal(calls[1].input, "/api/platform/jobs/123/cancel?user_id=7");
});

test("list platform getters compose required query params", async () => {
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

  await listPlatformJobs(7);
  await getPlatformJob(7, 123);
  await listPlatformJobItems(7, 123);
  await listPlatformJobEvents(7, 123, { afterId: 9, limit: 20 });
  await getPlatformQuota(7);

  assert.equal(calls[0].input, "/api/platform/jobs?user_id=7");
  assert.equal(calls[1].input, "/api/platform/jobs/123?user_id=7");
  assert.equal(calls[2].input, "/api/platform/jobs/123/items?user_id=7");
  assert.equal(calls[3].input, "/api/platform/jobs/123/events?user_id=7&after_id=9&limit=20");
  assert.equal(calls[4].input, "/api/platform/quota?user_id=7");
});

test("platform detail endpoints expose typed item, artifact and event fields", async () => {
  let callIndex = 0;
  Object.assign(globalThis, {
    fetch: async () => {
      callIndex += 1;
      if (callIndex === 1) {
        return createMockResponse({
          ok: true,
          body: {
            id: 123,
            job_type: "batch_mod_generation",
            status: "running",
            items: [
              {
                id: 1,
                item_index: 0,
                item_type: "card",
                status: "done",
                result_summary: "已创建",
                error_summary: "",
              },
            ],
            artifacts: [
              {
                id: 10,
                artifact_type: "image",
                file_name: "DarkBolt.png",
                result_summary: "已生成卡图",
              },
            ],
          },
        });
      }
      if (callIndex === 2) {
        return createMockResponse({
          ok: true,
          body: [
            {
              id: 1,
              item_index: 0,
              item_type: "card",
              status: "done",
              result_summary: "已创建",
              error_summary: "",
            },
          ],
        });
      }
      return createMockResponse({
        ok: true,
        body: [
          {
            event_id: 101,
            event_type: "job.started",
            job_id: 123,
            occurred_at: "2026-04-01T10:12:30+00:00",
            payload: { triggered_by: "user" },
            job_item_id: null,
            ai_execution_id: null,
          },
        ],
      });
    },
  });

  const detail = await getPlatformJob(7, 123);
  const items = await listPlatformJobItems(7, 123);
  const events = await listPlatformJobEvents(7, 123);

  assert.equal(detail.items?.[0].item_type, "card");
  assert.equal(detail.artifacts?.[0].artifact_type, "image");
  assert.equal(items[0].item_index, 0);
  assert.equal(events[0].event_type, "job.started");
});
