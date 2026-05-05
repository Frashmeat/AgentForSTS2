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

test("platform run flow creates non-generation job before confirming start", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  const progress: string[] = [];
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
    jobType: "log_analysis",
    workflowVersion: "2026.04.04",
    inputSummary: "analyze latest crash",
    createdFrom: "log_analysis",
    items: [],
    onProgress: (update) => progress.push(update.stage),
  });

  assert.equal(calls[0].input, "/api/me/jobs");
  assert.equal(calls[1].input, "/api/me/jobs/123/start");
  assert.equal(result.job.id, 123);
  assert.equal(result.started.status, "queued");
  assert.equal(result.startConfirmed, true);
  assert.deepEqual(progress, ["creating_job", "job_created", "starting_job", "queued"]);
});

test("platform run flow creates server workspace for generation jobs without explicit project root", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      if (calls.length === 1) {
        return createMockResponse({
          ok: true,
          body: {
            server_project_ref: "server-workspace:relic123",
            project_name: "FangedGrimoire",
            workspace_root: "F:/runtime/platform-workspaces/1001/relic123/FangedGrimoire",
            created_at: "2026-05-05T12:00:00+00:00",
          },
        });
      }
      if (calls.length === 2) {
        return createMockResponse({
          ok: true,
          body: { id: 124, status: "draft", job_type: "single_generate" },
        });
      }
      return createMockResponse({
        ok: true,
        body: { id: 124, status: "queued" },
      });
    },
  });

  await createAndStartPlatformFlow({
    jobType: "single_generate",
    workflowVersion: "2026.04.04",
    inputSummary: "relic:FangedGrimoire",
    createdFrom: "single_asset",
    items: [
      {
        item_type: "relic",
        input_summary: "FangedGrimoire",
        input_payload: {
          item_name: "FangedGrimoire",
          description: "每次造成伤害时获得 2 点格挡。",
          image_mode: "ai",
        },
      },
    ],
  });

  assert.equal(calls[0].input, "/api/me/server-workspaces");
  assert.match(String(calls[0].init?.body), /"project_name":"FangedGrimoire"/);
  assert.equal(calls[1].input, "/api/me/jobs");
  assert.match(String(calls[1].init?.body), /"server_project_ref":"server-workspace:relic123"/);
  assert.equal(calls[2].input, "/api/me/jobs/124/start");
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
            server_project_ref: "server-workspace:upload123",
            project_name: "DarkBlade",
            workspace_root: "F:/runtime/platform-workspaces/1001/upload123/DarkBlade",
            created_at: "2026-05-05T12:00:00+00:00",
          },
        });
      }
      if (calls.length === 2) {
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
      if (calls.length === 3) {
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

  assert.equal(calls[0].input, "/api/me/server-workspaces");
  assert.equal(calls[1].input, "/api/me/upload-assets");
  assert.equal(calls[2].input, "/api/me/jobs");
  assert.equal(calls[3].input, "/api/me/jobs/789/start");
  assert.match(String(calls[2].init?.body), /"server_project_ref":"server-workspace:upload123"/);
  assert.match(String(calls[2].init?.body), /"uploaded_asset_ref":"uploaded-asset:abc123"/);
  assert.equal(result.job.id, 789);
  assert.equal(result.started.status, "queued");
});

test("platform run flow creates server workspace before creating job and injects server_project_ref", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      if (calls.length === 1) {
        return createMockResponse({
          ok: true,
          body: {
            server_project_ref: "server-workspace:abc123",
            project_name: "DarkMod",
            workspace_root: "F:/runtime/platform-workspaces/1001/abc123/DarkMod",
            created_at: "2026-04-18T12:00:00+00:00",
          },
        });
      }
      if (calls.length === 2) {
        return createMockResponse({
          ok: true,
          body: { id: 790, status: "draft", job_type: "single_generate" },
        });
      }
      return createMockResponse({
        ok: true,
        body: { id: 790, status: "queued" },
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
        item_type: "custom_code",
        input_summary: "Dark Relic",
        input_payload: {
          asset_type: "custom_code",
          item_name: "SingleEffectPatch",
          description: "补一个单资产 custom_code 示例",
          image_mode: "ai",
        },
      },
    ],
    serverWorkspaceProjectName: "DarkMod",
  });

  assert.equal(calls[0].input, "/api/me/server-workspaces");
  assert.equal(calls[1].input, "/api/me/jobs");
  assert.equal(calls[2].input, "/api/me/jobs/790/start");
  assert.match(String(calls[1].init?.body), /"server_project_ref":"server-workspace:abc123"/);
  assert.equal(result.job.id, 790);
  assert.equal(result.started.status, "queued");
});
