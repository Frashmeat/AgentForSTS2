import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin server credentials page is routed and linked from admin surfaces", () => {
  const appSource = readSource("../src/App.tsx");
  const settingsSource = readSource("../src/components/SettingsPanel.tsx");
  const auditSource = readSource("../src/pages/AdminRuntimeAuditPage.tsx");

  assert.equal(existsSync(new URL("../src/pages/AdminServerCredentialsPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminServerCredentialsPage/);
  assert.match(appSource, /path="\/admin\/server-credentials"/);
  assert.match(settingsSource, /\/admin\/server-credentials/);
  assert.match(auditSource, /\/admin\/server-credentials/);
});

test("admin server credentials page uses credential APIs and hides raw secret values", () => {
  const pageSource = readSource("../src/pages/AdminServerCredentialsPage.tsx");

  assert.match(pageSource, /listAdminExecutionProfiles/);
  assert.match(pageSource, /listAdminServerCredentials/);
  assert.match(pageSource, /createAdminServerCredential/);
  assert.match(pageSource, /updateAdminServerCredential/);
  assert.match(pageSource, /enableAdminServerCredential/);
  assert.match(pageSource, /disableAdminServerCredential/);
  assert.match(pageSource, /runAdminServerCredentialHealthCheck/);
  assert.match(pageSource, /留空表示保留原值/);
  assert.doesNotMatch(pageSource, /credential_ciphertext/);
  assert.doesNotMatch(pageSource, /secret_ciphertext/);
});
