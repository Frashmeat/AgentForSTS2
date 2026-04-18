import test from "node:test";
import assert from "node:assert/strict";

import { createAndStartPlatformFlow } from "../src/features/platform-run/createAndStartFlow.ts";

interface MockResponseInit {
  ok: boolean;
  body?: unknown;
}

function createMockResponse(init: MockResponseInit) {
  return {
    ok: init.ok,
    async json() {
      return init.body;
    },
    async text() {
      return JSON.stringify(init.body ?? {});
    },
  };
}

test("platform run flow creates job before confirming start", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      if (calls.length === 1) {
        return createMockResponse({
          ok: true,
          body: { id: 123, status: "draft", job_type: "single_generate" },
        });
      }
      return createMockResponse({
        ok: true,
        body: { id: 123, status: "queued" },
      });
    },
  });

  const result = await createAndStartPlatformFlow({
    jobType: "single_generate",
    workflowVersion: "2026.04.04",
    inputSummary: "Dark Relic",
    createdFrom: "single_asset",
    items: [],
  });

  assert.equal(calls[0].input, "/api/me/jobs");
  assert.equal(calls[1].input, "/api/me/jobs/123/start");
  assert.equal(result.job.id, 123);
  assert.equal(result.started.status, "queued");
  assert.equal(result.startConfirmed, true);
});

test("platform run flow can stop after draft creation before start confirmation", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { id: 456, status: "draft", job_type: "log_analysis" },
      });
    },
  });

  const result = await createAndStartPlatformFlow({
    jobType: "log_analysis",
    workflowVersion: "2026.04.04",
    inputSummary: "analyze latest crash",
    createdFrom: "log_analysis",
    items: [],
    confirmStart: () => false,
  });

  assert.equal(calls.length, 1);
  assert.equal(calls[0].input, "/api/me/jobs");
  assert.equal(result.job.id, 456);
  assert.equal(result.started, null);
  assert.equal(result.startConfirmed, false);
});

test("platform run flow uploads assets before creating job and injects uploaded_asset_ref", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      if (calls.length === 1) {
        return createMockResponse({
          ok: true,
          body: {
            uploaded_asset_ref: "uploaded-asset:abc123",
            file_name: "dark-blade.png",
            mime_type: "image/png",
            size_bytes: 16,
            created_at: "2026-04-18T12:00:00+00:00",
          },
        });
      }
      if (calls.length === 2) {
        return createMockResponse({
          ok: true,
          body: { id: 789, status: "draft", job_type: "single_generate" },
        });
      }
      return createMockResponse({
        ok: true,
        body: { id: 789, status: "queued" },
      });
    },
  });

  const result = await createAndStartPlatformFlow({
    jobType: "single_generate",
    workflowVersion: "2026.04.04",
    inputSummary: "Dark Relic",
    createdFrom: "single_asset",
    items: [
      {
        item_type: "card",
        input_summary: "Dark Relic",
        input_payload: {
          asset_type: "card",
          item_name: "DarkBlade",
          description: "1 费攻击牌，造成 8 点伤害。",
          image_mode: "upload",
        },
      },
    ],
    serverUploads: [
      {
        itemIndex: 0,
        fileName: "dark-blade.png",
        contentBase64: "ZmFrZS1pbWFnZS1ieXRlcw==",
        mimeType: "image/png",
      },
    ],
  });

  assert.equal(calls[0].input, "/api/me/upload-assets");
  assert.equal(calls[1].input, "/api/me/jobs");
  assert.equal(calls[2].input, "/api/me/jobs/789/start");
  assert.match(String(calls[1].init?.body), /"uploaded_asset_ref":"uploaded-asset:abc123"/);
  assert.equal(result.job.id, 789);
  assert.equal(result.started.status, "queued");
});
