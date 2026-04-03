import test from "node:test";
import assert from "node:assert/strict";

import { resolveDefaultProjectRootValue } from "../src/shared/useDefaultProjectRoot.ts";

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
