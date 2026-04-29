import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("execution mode dialog covers local server and login states", () => {
  const source = readSource("../src/components/ExecutionModeDialog.tsx");

  assert.match(source, /本机执行/);
  assert.match(source, /服务器模式/);
  assert.match(source, /onGoLogin/);
  assert.match(source, /用户中心/);
  assert.match(source, /localUnavailableReasons/);
  assert.match(source, /localUnavailableReasons\.map/);
});

test("execution flow does not use native browser confirm", () => {
  const source = readSource("../src/features/workspace/useExecutionModeFlow.ts");

  assert.doesNotMatch(source, /window\.confirm/);
});
