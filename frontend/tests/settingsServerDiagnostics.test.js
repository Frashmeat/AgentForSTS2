import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("settings panel exposes queue worker diagnostics in server tab", () => {
  const source = readSource("../src/components/SettingsPanel.tsx");

  assert.match(source, /loadPlatformQueueWorkerStatus/);
  assert.match(source, /平台队列 Worker 诊断/);
  assert.match(source, /Leader 概览/);
  assert.match(source, /切换窗口/);
  assert.match(source, /最近 Leader 事件/);
  assert.match(source, /当前有效 Leader/);
  assert.match(source, /成为 Leader/);
  assert.match(source, /等待 Failover/);
});
