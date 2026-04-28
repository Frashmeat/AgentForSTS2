import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin executions page is routed and linked from admin audit page", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");

  assert.equal(existsSync(new URL("../src/pages/admin/AdminExecutionsPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminExecutionsPage/);
  assert.match(appSource, /path="executions"/);
  assert.match(layoutSource, /\/admin\/executions/);
});

test("admin executions page uses execution list and detail APIs", () => {
  const pageSource = readSource("../src/pages/admin/AdminExecutionsPage.tsx");

  assert.match(pageSource, /listAdminJobExecutions/);
  assert.match(pageSource, /getAdminExecution/);
  assert.match(pageSource, /任务编号/);
  assert.match(pageSource, /执行编号/);
  assert.match(pageSource, /服务商/);
  assert.match(pageSource, /请求标识/);
  assert.match(pageSource, /技术信息/);
  assert.match(pageSource, /执行详情/);
});
