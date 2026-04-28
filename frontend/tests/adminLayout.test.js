import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin layout and overview are routed under /admin", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");
  const overviewSource = readSource("../src/pages/admin/AdminOverviewPage.tsx");

  assert.equal(existsSync(new URL("../src/pages/admin/AdminLayout.tsx", import.meta.url)), true);
  assert.equal(existsSync(new URL("../src/pages/admin/AdminOverviewPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminLayout/);
  assert.match(appSource, /AdminOverviewPage/);
  assert.match(appSource, /path="\/admin"/);
  assert.match(layoutSource, /管理台首页/);
  assert.match(layoutSource, /运行状态/);
  assert.match(layoutSource, /执行记录/);
  assert.match(layoutSource, /审计事件/);
  assert.match(layoutSource, /执行配置/);
  assert.match(layoutSource, /服务器凭据/);
  assert.match(layoutSource, /健康检查/);
  assert.match(layoutSource, /退款记录/);
  assert.match(layoutSource, /用户与额度/);
  assert.match(overviewSource, /listAdminExecutionProfiles/);
  assert.match(overviewSource, /listAdminServerCredentials/);
});
