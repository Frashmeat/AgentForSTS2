import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("batch runtime model keeps workflow error naming separate from page-level global error", () => {
  const stateSource = readSource("../src/features/batch-generation/state.ts");
  const pageSource = readSource("../src/features/batch-generation/view.tsx");

  assert.match(stateSource, /\bworkflowErrorMessage\b/);
  assert.match(pageSource, /\bglobalError\b/);
  assert.doesNotMatch(stateSource, /\bglobalError\b/);
  assert.doesNotMatch(pageSource, /\bruntimeState\.globalError\b/);
});
