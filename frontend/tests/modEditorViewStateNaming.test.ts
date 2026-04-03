import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("ModEditorFeatureView uses explicit error message state names", () => {
  const source = readSource("../src/features/mod-editor/view.tsx");

  assert.match(source, /\banalysisErrorMessage\b/);
  assert.match(source, /\bmodifyErrorMessage\b/);
  assert.doesNotMatch(source, /\banalysisError\b/);
  assert.doesNotMatch(source, /\bmodifyError\b/);
});
