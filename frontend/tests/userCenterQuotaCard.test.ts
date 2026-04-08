import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("quota card shows fixed pool remaining and refunded summary", () => {
  const quotaCardSource = readSource("../src/features/user-center/quotaCard.tsx");

  assert.match(quotaCardSource, /服务器次数池/);
  assert.match(quotaCardSource, /剩余次数/);
  assert.match(quotaCardSource, /已返还/);
  assert.doesNotMatch(quotaCardSource, /日额度/);
  assert.doesNotMatch(quotaCardSource, /周额度/);
});
