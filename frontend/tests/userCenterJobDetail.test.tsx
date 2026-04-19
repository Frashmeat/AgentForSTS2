import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("job detail page loads detail items and refund summary", () => {
  const pageSource = readSource("../src/features/user-center/job-detail-page.tsx");
  const summarySource = readSource("../src/features/user-center/refundSummary.tsx");

  assert.match(pageSource, /getMyJob/);
  assert.match(pageSource, /listMyJobItems/);
  assert.match(pageSource, /detail\.artifacts/);
  assert.match(pageSource, /构建产物/);
  assert.match(pageSource, /build_output/);
  assert.match(pageSource, /deployed_output/);
  assert.match(pageSource, /artifact\.object_key/);
  assert.match(pageSource, /部署位置/);
  assert.match(pageSource, /来源：/);
  assert.match(pageSource, /RefundSummary/);
  assert.match(pageSource, /to=\"\/\"/);
  assert.match(pageSource, /返回首页/);
  assert.match(summarySource, /original_deducted/);
  assert.match(summarySource, /refunded_amount/);
  assert.match(summarySource, /net_consumed/);
  assert.match(summarySource, /refund_reason_summary/);
});
