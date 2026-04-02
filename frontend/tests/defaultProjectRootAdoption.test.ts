import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("default project root loading is shared across app entry points", () => {
  const appSource = readSource("../src/App.tsx");
  const batchModeSource = readSource("../src/pages/BatchMode.tsx");
  const modEditorSource = readSource("../src/features/mod-editor/view.tsx");

  assert.match(appSource, /useDefaultProjectRoot/);
  assert.match(batchModeSource, /useDefaultProjectRoot/);
  assert.match(modEditorSource, /useDefaultProjectRoot/);
});
