import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App delegates execution mode state and actions to useExecutionModeFlow", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /useExecutionModeFlow/);
  assert.doesNotMatch(appSource, /loadLocalAiCapabilityStatus/);
  assert.doesNotMatch(appSource, /createAndStartPlatformFlow/);
  assert.doesNotMatch(appSource, /const \[pendingExecution, setPendingExecution]/);
});

test("useExecutionModeFlow owns capability probing and dialog state", () => {
  const flowSource = readSource("../src/features/workspace/useExecutionModeFlow.ts");

  assert.match(flowSource, /loadLocalAiCapabilityStatus/);
  assert.match(flowSource, /createAndStartPlatformFlow/);
  assert.match(flowSource, /pendingExecution/);
  assert.match(flowSource, /handleExecutionRequest/);
  assert.match(flowSource, /handleChooseServerExecution/);
});

test("execution mode errors use the shared status notice stack instead of native alerts", () => {
  const appSource = readSource("../src/App.tsx");
  const flowSource = readSource("../src/features/workspace/useExecutionModeFlow.ts");

  assert.match(appSource, /StatusNoticeStack/);
  assert.match(appSource, /showStatusNotice/);
  assert.match(flowSource, /onStatusNotice/);
  assert.match(flowSource, /tone:\s*"error"/);
  assert.doesNotMatch(flowSource, /window\.alert/);
});
