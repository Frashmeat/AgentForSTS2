import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin runtime audit page uses admin audit api and exposes runtime event copy", () => {
  const pageSource = readSource("../src/pages/AdminRuntimeAuditPage.tsx");
  const appSource = readSource("../src/App.tsx");
  const settingsSource = readSource("../src/components/SettingsPanel.tsx");

  assert.match(pageSource, /listAdminAuditEvents/);
  assert.match(pageSource, /job_id = 0/);
  assert.match(pageSource, /运行时审计/);
  assert.match(pageSource, /仅 queue worker/);
  assert.match(pageSource, /事件类型统计/);
  assert.match(pageSource, /DEFAULT_AUDIT_LIMIT/);
  assert.match(appSource, /path="\/admin\/runtime-audit"/);
  assert.match(settingsSource, /打开管理员审计页/);
});
