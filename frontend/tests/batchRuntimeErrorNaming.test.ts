import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("batch runtime model and feature view use explicit workflow error naming", () => {
  const stateSource = readSource("../src/features/batch-generation/state.ts");
  const pageSource = readSource("../src/features/batch-generation/view.tsx");

  assert.match(stateSource, /\bworkflowErrorMessage\b/);
  assert.match(pageSource, /\bworkflowErrorMessage\b/);
  assert.doesNotMatch(stateSource, /\bglobalError\b/);
  assert.doesNotMatch(pageSource, /\bglobalError\b/);
});
