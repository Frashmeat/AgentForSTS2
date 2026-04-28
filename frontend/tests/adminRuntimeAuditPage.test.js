import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin runtime audit page uses admin audit api and exposes runtime event copy", () => {
  const auditSource = readSource("../src/pages/admin/AdminAuditPage.tsx");
  const runtimeSource = readSource("../src/pages/admin/AdminRuntimePage.tsx");
  const appSource = readSource("../src/App.tsx");

  assert.match(auditSource, /listAdminAuditEvents/);
  assert.match(auditSource, /审计事件/);
  assert.match(auditSource, /仅队列 Worker/);
  assert.match(auditSource, /DEFAULT_AUDIT_LIMIT/);
  assert.match(runtimeSource, /loadPlatformQueueWorkerStatus/);
  assert.match(runtimeSource, /运行状态/);
  assert.match(appSource, /path="runtime"/);
  assert.match(appSource, /path="audit"/);
  assert.match(appSource, /path="\/admin\/runtime-audit"/);
  assert.match(appSource, /to="\/admin\/audit"/);
});
