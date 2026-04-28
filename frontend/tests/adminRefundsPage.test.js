import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin refunds page is routed and linked from admin audit page", () => {
  const appSource = readSource("../src/App.tsx");
  const auditSource = readSource("../src/pages/AdminRuntimeAuditPage.tsx");

  assert.equal(existsSync(new URL("../src/pages/AdminRefundsPage.tsx", import.meta.url)), true);
  assert.match(appSource, /AdminRefundsPage/);
  assert.match(appSource, /path="\/admin\/refunds"/);
  assert.match(auditSource, /\/admin\/refunds/);
});

test("admin refunds page uses refund API and user_id filter", () => {
  const pageSource = readSource("../src/pages/AdminRefundsPage.tsx");

  assert.match(pageSource, /listAdminQuotaRefunds/);
  assert.match(pageSource, /user_id/);
  assert.match(pageSource, /refund_reason/);
  assert.match(pageSource, /charge_status/);
});
