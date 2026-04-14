import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("batch feature opens planning through shared planning session helper", () => {
  const source = readSource("../src/features/batch-generation/view.tsx");

  assert.match(source, /openBatchPlanningSocket/);
  assert.match(source, /startPlanning/);
  assert.match(source, /start_with_plan/);
});
