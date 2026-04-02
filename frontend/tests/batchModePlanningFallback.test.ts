import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("BatchMode wires HTTP planning fallback through shared workflow api", () => {
  const source = readSource("../src/pages/BatchMode.tsx");

  assert.match(source, /generateModPlan/);
  assert.match(source, /快速规划（HTTP）/);
  assert.match(source, /startPlanningFallback/);
});
