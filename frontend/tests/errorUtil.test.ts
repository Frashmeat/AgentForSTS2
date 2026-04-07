import test from "node:test";
import assert from "node:assert/strict";

import { resolveErrorMessage, resolveWorkflowErrorMessage } from "../src/shared/error.ts";

test("resolveErrorMessage returns Error.message for Error objects", () => {
  assert.equal(resolveErrorMessage(new Error("boom")), "boom");
});

test("resolveErrorMessage stringifies non-Error values", () => {
  assert.equal(resolveErrorMessage("plain"), "plain");
  assert.equal(resolveErrorMessage(42), "42");
});

test("resolveErrorMessage prefers structured envelope message", () => {
  assert.equal(
    resolveErrorMessage({
      error: {
        code: "auth_required",
        message: "请先登录后再继续",
        detail: "authentication required",
      },
    }),
    "请先登录后再继续",
  );
});

test("resolveErrorMessage avoids exposing raw object strings for unknown values", () => {
  assert.equal(
    resolveErrorMessage({ foo: "bar" }),
    "请求失败，请稍后重试",
  );
});

test("resolveWorkflowErrorMessage prefers shared workflow fields", () => {
  assert.equal(
    resolveWorkflowErrorMessage({
      message: "构建失败",
      detail: "dotnet publish exited with code 1",
      code: "build_failed",
    }),
    "构建失败",
  );
});
