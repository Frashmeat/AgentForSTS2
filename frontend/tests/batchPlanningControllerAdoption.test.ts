import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("BatchMode uses shared planning controller for websocket and HTTP planning", () => {
  const source = readSource("../src/pages/BatchMode.tsx");

  assert.match(source, /createBatchPlanningController/);
  assert.match(source, /startSocketPlanning/);
  assert.match(source, /startHttpPlanning/);
});
