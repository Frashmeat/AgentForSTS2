import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("BuildDeploy uses shared controller for action flow", () => {
  const source = readSource("../src/components/BuildDeploy.tsx");

  assert.match(source, /createBuildDeployController/);
});
