import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("BatchMode exposes create project action wired to workflow api", () => {
  const source = readSource("../src/pages/BatchMode.tsx");

  assert.match(source, /createProject/);
  assert.match(source, /deriveCreateProjectRequest/);
  assert.match(source, /创建项目/);
  assert.match(source, /createProjectBusy/);
});
