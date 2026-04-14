import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("app and feature views expose server flow gate hooks", () => {
  const appSource = readSource("../src/App.tsx");
  const flowSource = readSource("../src/features/workspace/useExecutionModeFlow.ts");
  const batchSource = readSource("../src/features/batch-generation/view.tsx");
  const modSource = readSource("../src/features/mod-editor/view.tsx");
  const logSource = readSource("../src/features/log-analysis/view.tsx");

  assert.match(appSource, /ExecutionModeDialog/);
  assert.match(appSource, /useExecutionModeFlow/);
  assert.match(flowSource, /createAndStartPlatformFlow/);
  assert.match(flowSource, /loadLocalAiCapabilityStatus/);
  assert.match(batchSource, /onRequestExecution/);
  assert.match(modSource, /onRequestExecution/);
  assert.match(logSource, /onRequestExecution/);
});
