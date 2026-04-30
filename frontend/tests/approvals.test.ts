import test from "node:test";
import assert from "node:assert/strict";

import {
  approveApproval,
  describeApprovalPayload,
  getApproval,
  listApprovals,
  summarizeApprovalPending,
  type ApprovalRequest,
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

function createApproval(overrides: Partial<ApprovalRequest> = {}): ApprovalRequest {
  return {
    action_id: "req-1",
    kind: "shell_command",
    title: "run build",
    reason: "verify",
    risk_level: "medium",
    requires_approval: true,
    status: "pending",
    payload: {},
    ...overrides,
  };
}

function setWorkstationApiBase() {
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      workstation: "http://127.0.0.1:7860",
    },
  });
}

test("summarizeApprovalPending prefers backend summary", () => {
  assert.equal(summarizeApprovalPending("需要先审批", [createApproval()]), "需要先审批");
});

test("summarizeApprovalPending falls back to request count", () => {
  assert.equal(
    summarizeApprovalPending("", [createApproval(), createApproval({ action_id: "req-2" })]),
    "有 2 个动作等待审批",
  );
});

test("describeApprovalPayload prefers command then path", () => {
  assert.equal(
    describeApprovalPayload(createApproval({ payload: { command: ["dotnet", "publish"] } })),
    "dotnet publish",
  );
  assert.equal(
    describeApprovalPayload(createApproval({ payload: { path: "Cards/TestCard.cs" } })),
    "Cards/TestCard.cs",
  );
});

test("listApprovals requests all approvals with GET", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: [
          {
            action_id: "req-1",
            kind: "shell_command",
            title: "run build",
            reason: "verify",
            risk_level: "medium",
            requires_approval: true,
            status: "pending",
            payload: {},
          },
        ],
      });
    },
  });

  const approvals = await listApprovals();

  assert.equal(calls.length, 1);
  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/approvals");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(approvals[0].action_id, "req-1");
});

test("getApproval requests one approval by id", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: {
          action_id: "req-2",
          kind: "shell_command",
          title: "deploy",
          reason: "publish",
          risk_level: "high",
          requires_approval: true,
          status: "approved",
          payload: {},
          source_backend: "codex",
          created_at: "2026-04-01T10:12:30.000000+00:00",
        },
      });
    },
  });

  const approval = await getApproval("req-2");

  assert.equal(calls.length, 1);
  assert.equal(calls[0].input, "http://127.0.0.1:7860/api/approvals/req-2");
  assert.equal(calls[0].init?.method, "GET");
  assert.equal(approval.source_backend, "codex");
  assert.equal(approval.created_at, "2026-04-01T10:12:30.000000+00:00");
});

test("approveApproval throws response text when request fails", async () => {
  setWorkstationApiBase();
  Object.assign(globalThis, {
    fetch: async () =>
      createMockResponse({
        ok: false,
        text: '{"detail":"Approval request not found"}',
      }),
  });

  await assert.rejects(() => approveApproval("missing-id"), /Approval request not found/);
});
