import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("batch runtime state and page use explicit workflow error naming", () => {
  const stateSource = readSource("../src/features/batch-generation/state.ts");
  const pageSource = readSource("../src/pages/BatchMode.tsx");

  assert.match(stateSource, /\bworkflowErrorMessage\b/);
  assert.match(pageSource, /\bruntimeState\.workflowErrorMessage\b/);
  assert.doesNotMatch(stateSource, /\bglobalError\b/);
  assert.doesNotMatch(pageSource, /\bruntimeState\.globalError\b/);
});
