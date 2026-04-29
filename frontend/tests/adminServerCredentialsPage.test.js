import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin server credentials page is routed and linked from admin surfaces", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");

  assert.equal(existsSync(new URL("../src/pages/admin/AdminServerCredentialsPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminServerCredentialsPage/);
  assert.match(appSource, /path="server-credentials"/);
  assert.match(layoutSource, /\/admin\/server-credentials/);
});

test("admin server credentials page uses credential APIs and hides raw secret values", () => {
  const pageSource = readSource("../src/pages/admin/AdminServerCredentialsPage.tsx");

  assert.match(pageSource, /listAdminExecutionProfiles/);
  assert.match(pageSource, /listAdminServerCredentials/);
  assert.match(pageSource, /createAdminServerCredential/);
  assert.match(pageSource, /updateAdminServerCredential/);
  assert.match(pageSource, /enableAdminServerCredential/);
  assert.match(pageSource, /disableAdminServerCredential/);
  assert.match(pageSource, /runAdminServerCredentialHealthCheck/);
  assert.match(pageSource, /留空表示保留原值/);
  assert.match(pageSource, /formatAdminProvider/);
  assert.match(pageSource, /formatAdminAuthType/);
  assert.match(pageSource, /formatAdminStatus/);
  assert.doesNotMatch(pageSource, /credential_ciphertext/);
  assert.doesNotMatch(pageSource, /secret_ciphertext/);
  assert.doesNotMatch(pageSource, /deleteAdminServerCredential/);
});

test("admin server credentials page keeps credentials non-deletable and uses denser layout", () => {
  const pageSource = readSource("../src/pages/admin/AdminServerCredentialsPage.tsx");

  assert.match(pageSource, /max-w-7xl/);
  assert.match(pageSource, /xl:grid-cols-\[420px_minmax\(0,1fr\)\]/);
  assert.match(pageSource, /当前筛选条件下没有服务器凭据/);
  assert.doesNotMatch(pageSource, /确认删除服务器凭据/);
  assert.doesNotMatch(pageSource, /确认删除/);
  assert.doesNotMatch(pageSource, /Trash2/);
  assert.doesNotMatch(pageSource, /window\.confirm/);
});
