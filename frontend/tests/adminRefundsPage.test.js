import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin refunds page is routed and linked from admin audit page", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");

  assert.equal(existsSync(new URL("../src/pages/admin/AdminRefundsPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminRefundsPage/);
  assert.match(appSource, /path="refunds"/);
  assert.match(layoutSource, /\/admin\/refunds/);
});

test("admin refunds page uses refund API and business field labels", () => {
  const pageSource = readSource("../src/pages/admin/AdminRefundsPage.tsx");

  assert.match(pageSource, /listAdminQuotaRefunds/);
  assert.match(pageSource, /用户编号/);
  assert.match(pageSource, /用户/);
  assert.match(pageSource, /执行编号/);
  assert.match(pageSource, /状态/);
  assert.match(pageSource, /原因/);
  assert.match(pageSource, /额度/);
  assert.match(pageSource, /时间/);
  assert.match(pageSource, /formatAdminRefundReason/);
});
