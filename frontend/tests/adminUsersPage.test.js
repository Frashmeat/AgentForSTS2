import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin users page is a routed extension placeholder without fake operations", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");
  const pageSource = readSource("../src/pages/admin/AdminUsersPage.tsx");

  assert.equal(existsSync(new URL("../src/pages/admin/AdminUsersPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminUsersPage/);
  assert.match(appSource, /path="users"/);
  assert.match(layoutSource, /\/admin\/users/);
  assert.match(pageSource, /用户与额度/);
  assert.match(pageSource, /待接入/);
  assert.match(pageSource, /没有管理员用户列表或额度调整接口/);
  assert.doesNotMatch(pageSource, /fetch\(/);
});
