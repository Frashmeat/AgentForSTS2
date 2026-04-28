import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel no longer exposes admin maintenance diagnostics", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.doesNotMatch(source, /loadPlatformQueueWorkerStatus/);
  assert.doesNotMatch(source, /平台队列 Worker 诊断/);
  assert.doesNotMatch(source, /服务器凭据管理/);
  assert.doesNotMatch(source, /打开管理员审计页/);
  assert.doesNotMatch(source, /\/admin\/runtime-audit/);
  assert.doesNotMatch(source, /\/admin\/server-credentials/);
});

test("queue worker diagnostics API remains available for admin runtime page", () => {
  const source = readSource("../src/shared/api/config.ts");

  assert.match(source, /\/api\/platform\/queue-worker-status/);
  assert.match(source, /backend:\s*"web"/);
});
