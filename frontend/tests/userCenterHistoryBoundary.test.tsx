import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("user center history stays isolated from local recovery sources", () => {
  const pageSource = readSource("../src/features/user-center/page.tsx");
  const historySource = readSource("../src/features/user-center/historyList.tsx");

  assert.doesNotMatch(pageSource, /localStorage/);
  assert.doesNotMatch(pageSource, /loadSingleAssetSnapshot/);
  assert.doesNotMatch(historySource, /localStorage/);
  assert.doesNotMatch(historySource, /single-asset\/recovery/);
  assert.match(historySource, /平台任务/);
  assert.match(historySource, /resolveDeliveryBadge/);
  assert.match(historySource, /delivery_state/);
  assert.match(historySource, /已部署/);
  assert.match(historySource, /已构建/);
});
