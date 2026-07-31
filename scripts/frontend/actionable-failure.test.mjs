import assert from "node:assert/strict";
import { after, test } from "node:test";

import { createServer } from "vite";

const vite = await createServer({
  appType: "custom",
  server: { middlewareMode: true },
});

after(async () => {
  await vite.close();
});

const { isActionableFailure, toActionableFailure } = await vite.ssrLoadModule(
  "/src/services/actionableFailure.ts",
);

const valid = {
  schemaVersion: 1,
  code: "llm.rate_limited",
  category: "rate_limit",
  stage: "llm.complete",
  message: "Retry later.",
  action: "retry",
  retryable: true,
  retryAfterMs: 17000,
  context: { httpStatus: 429 },
  diagnostic: {
    id: "diag-safe",
    summary: "Provider response was not exposed.",
    ioKind: "other",
  },
};

test("accepts the complete Core failure shape", () => {
  assert.equal(isActionableFailure(valid), true);
  assert.equal(toActionableFailure(valid), valid);
});

test("rejects stale and malformed failure shapes", () => {
  assert.equal(isActionableFailure({ ...valid, schemaVersion: 2 }), false);
  assert.equal(isActionableFailure({ ...valid, category: "fatal" }), false);
  assert.equal(isActionableFailure({ ...valid, action: "restart_everything" }), false);
  assert.equal(isActionableFailure({ ...valid, retryable: "yes" }), false);
  assert.equal(isActionableFailure({ ...valid, diagnostic: { id: "x" } }), false);
  assert.equal(isActionableFailure({ ...valid, leakedBody: "token=secret" }), false);
  assert.equal(
    isActionableFailure({
      ...valid,
      diagnostic: { ...valid.diagnostic, id: "C:/Users/private" },
    }),
    false,
  );
  assert.equal(
    isActionableFailure({
      ...valid,
      context: { projectRelativePath: "../outside.txt" },
    }),
    false,
  );
});

test("malformed rejects become a safe fallback without raw text", () => {
  const canary = "C:/Users/private/file?token=secret";
  const failure = toActionableFailure(canary);
  assert.equal(failure.code, "core.unclassified");
  assert.equal(failure.stage, "client.invoke");
  assert.equal(failure.retryable, true);
  assert.equal(JSON.stringify(failure).includes(canary), false);
  assert.equal(JSON.stringify(failure).includes("secret"), false);
});
