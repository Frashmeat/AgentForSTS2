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
});
