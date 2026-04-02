import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("ModEditorFeatureView uses shared controllers for analysis and modify flows", () => {
  const source = readSource("../src/features/mod-editor/view.tsx");

  assert.match(source, /createModEditorAnalysisController/);
  assert.match(source, /createModEditorModifyController/);
  assert.doesNotMatch(source, /new ModAnalysisSocket/);
  assert.doesNotMatch(source, /new WorkflowSocket/);
});
