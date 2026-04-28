import test from "node:test";
import assert from "node:assert/strict";

import {
  formatAdminAuthType,
  formatAdminEventType,
  formatAdminProvider,
  formatAdminRefundReason,
  formatAdminStatus,
} from "../src/pages/admin/adminDisplay.ts";

test("admin display formats status values into business labels and tones", () => {
  assert.deepEqual(formatAdminStatus("healthy"), { label: "健康", tone: "success" });
  assert.deepEqual(formatAdminStatus("degraded"), { label: "需复检", tone: "warning" });
  assert.deepEqual(formatAdminStatus("auth_failed"), { label: "认证失败", tone: "danger" });
  assert.deepEqual(formatAdminStatus("quota_exhausted"), { label: "额度耗尽", tone: "danger" });
  assert.deepEqual(formatAdminStatus("retrying"), { label: "重试中", tone: "warning" });
});

test("admin display formats event types into operation labels", () => {
  assert.equal(formatAdminEventType("runtime.queue_worker.leader_renewed"), "调度权续租");
  assert.equal(formatAdminEventType("runtime.queue_worker.job_claimed"), "领取任务");
  assert.equal(formatAdminEventType("runtime.queue_worker.workspace_locked"), "工作区占用");
  assert.equal(formatAdminEventType("quota.refunded"), "额度返还");
});

test("admin display formats auth types, providers and refund reasons", () => {
  assert.equal(formatAdminAuthType("api_key"), "密钥");
  assert.equal(formatAdminAuthType("ak_sk"), "访问密钥");
  assert.equal(formatAdminProvider("openai"), "OpenAI");
  assert.equal(formatAdminProvider("anthropic"), "Anthropic");
  assert.equal(formatAdminRefundReason("execution_failed"), "执行失败");
  assert.equal(formatAdminRefundReason("credential_failed"), "凭据失败");
});
