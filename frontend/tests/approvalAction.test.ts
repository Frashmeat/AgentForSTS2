import test from "node:test";
import assert from "node:assert/strict";

import { runApprovalAction } from "../src/shared/approvalAction.ts";

test("runApprovalAction toggles busy state and forwards success result", async () => {
  const events: string[] = [];
  const result = await runApprovalAction({
    actionId: "approve-1",
    action: async (actionId) => {
      events.push(`action:${actionId}`);
      return { action_id: actionId, status: "approved" };
    },
    onBusyChange(actionId) {
      events.push(`busy:${actionId ?? "null"}`);
    },
    onSuccess(updated) {
      events.push(`success:${updated.status}`);
    },
    onError(message) {
      events.push(`error:${message}`);
    },
  });

  assert.deepEqual(events, ["busy:approve-1", "action:approve-1", "success:approved", "busy:null"]);
  assert.deepEqual(result, { action_id: "approve-1", status: "approved" });
});

test("runApprovalAction toggles busy state and forwards normalized error", async () => {
  const events: string[] = [];

  const result = await runApprovalAction({
    actionId: "reject-1",
    action: async () => {
      throw new Error("approval failed");
    },
    onBusyChange(actionId) {
      events.push(`busy:${actionId ?? "null"}`);
    },
    onSuccess() {
      events.push("success");
    },
    onError(message) {
      events.push(`error:${message}`);
    },
  });

  assert.equal(result, undefined);
  assert.deepEqual(events, ["busy:reject-1", "error:approval failed", "busy:null"]);
});
