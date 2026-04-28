import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin executions page is routed and linked from admin audit page", () => {
  const appSource = readSource("../src/App.tsx");
  const auditSource = readSource("../src/pages/AdminRuntimeAuditPage.tsx");

  assert.equal(existsSync(new URL("../src/pages/AdminExecutionsPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminExecutionsPage/);
  assert.match(appSource, /path="\/admin\/executions"/);
  assert.match(auditSource, /\/admin\/executions/);
});

test("admin executions page uses execution list and detail APIs", () => {
  const pageSource = readSource("../src/pages/AdminExecutionsPage.tsx");

  assert.match(pageSource, /listAdminJobExecutions/);
  assert.match(pageSource, /getAdminExecution/);
  assert.match(pageSource, /job_id/);
  assert.match(pageSource, /执行详情/);
});
