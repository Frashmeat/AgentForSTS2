import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin credential health page is routed and summarizes health states", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");
  const pageSource = readSource("../src/pages/admin/AdminCredentialHealthPage.tsx");

  assert.equal(existsSync(new URL("../src/pages/admin/AdminCredentialHealthPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminCredentialHealthPage/);
  assert.match(appSource, /path="credential-health"/);
  assert.match(layoutSource, /\/admin\/credential-health/);
  assert.match(pageSource, /listAdminServerCredentials/);
  assert.match(pageSource, /runAdminServerCredentialHealthCheck/);
  assert.match(pageSource, /健康/);
  assert.match(pageSource, /需复检/);
  assert.match(pageSource, /认证失败/);
  assert.match(pageSource, /调用限流/);
  assert.match(pageSource, /额度耗尽/);
  assert.match(pageSource, /已停用/);
});
