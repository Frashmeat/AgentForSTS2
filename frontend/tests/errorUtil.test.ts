import test from "node:test";
import assert from "node:assert/strict";

import { resolveErrorMessage } from "../src/shared/error.ts";

test("resolveErrorMessage returns Error.message for Error objects", () => {
  assert.equal(resolveErrorMessage(new Error("boom")), "boom");
});

test("resolveErrorMessage stringifies non-Error values", () => {
  assert.equal(resolveErrorMessage("plain"), "plain");
  assert.equal(resolveErrorMessage(42), "42");
});
