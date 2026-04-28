import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin execution profiles page is routed under admin layout", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");
  const pageSource = readSource("../src/pages/admin/AdminExecutionProfilesPage.tsx");

  assert.equal(existsSync(new URL("../src/pages/admin/AdminExecutionProfilesPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminExecutionProfilesPage/);
  assert.match(appSource, /path="execution-profiles"/);
  assert.match(layoutSource, /\/admin\/execution-profiles/);
  assert.match(pageSource, /listAdminExecutionProfiles/);
  assert.match(pageSource, /listAdminServerCredentials/);
  assert.match(pageSource, /编号/);
  assert.match(pageSource, /名称/);
  assert.match(pageSource, /服务商/);
  assert.match(pageSource, /模型/);
  assert.match(pageSource, /可用凭据/);
  assert.match(pageSource, /用户可选状态/);
});
