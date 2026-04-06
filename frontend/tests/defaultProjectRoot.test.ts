import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { resolveDefaultProjectRootValue } from "../src/shared/useDefaultProjectRoot.ts";

function readHookSource() {
  return readFileSync(new URL("../src/shared/useDefaultProjectRoot.ts", import.meta.url), "utf8");
}

test("resolveDefaultProjectRootValue keeps existing input when already filled", () => {
  assert.equal(
    resolveDefaultProjectRootValue("E:/MyExistingMod", "E:/DefaultRoot"),
    "E:/MyExistingMod",
  );
});

test("resolveDefaultProjectRootValue falls back to config default when current is empty", () => {
  assert.equal(
    resolveDefaultProjectRootValue("", "E:/DefaultRoot"),
    "E:/DefaultRoot",
  );
});

test("resolveDefaultProjectRootValue keeps current value when config default is missing", () => {
  assert.equal(
    resolveDefaultProjectRootValue("",
      undefined),
    "",
  );
});

test("useDefaultProjectRoot keeps config loading independent from callback identity", () => {
  const source = readHookSource();

  assert.match(source, /useRef/);
  assert.doesNotMatch(source, /\[onConfigLoaded,\s*setProjectRoot\]/);
});
