import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin runtime and audit routes are split under admin layout", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");
  const auditSource = readSource("../src/pages/admin/AdminAuditPage.tsx");
  const runtimeSource = readSource("../src/pages/admin/AdminRuntimePage.tsx");

  assert.match(appSource, /AdminRuntimePage/);
  assert.match(appSource, /AdminAuditPage/);
  assert.match(appSource, /path="runtime"/);
  assert.match(appSource, /path="audit"/);
  assert.match(appSource, /to="\/admin\/audit"/);
  assert.match(layoutSource, /\/admin\/runtime/);
  assert.match(layoutSource, /\/admin\/audit/);
  assert.match(runtimeSource, /loadPlatformQueueWorkerStatus/);
  assert.match(auditSource, /listAdminAuditEvents/);
  assert.match(auditSource, /formatAdminEventType/);
});
