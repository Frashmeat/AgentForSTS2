import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("user center page composes profile quota and jobs overview", () => {
  const pageSource = readSource("../src/features/user-center/page.tsx");
  const modelSource = readSource("../src/features/user-center/model.ts");

  assert.match(pageSource, /QuotaCard/);
  assert.match(pageSource, /HistoryList/);
  assert.match(modelSource, /loadUserCenterOverview/);
  assert.match(modelSource, /getMyProfile/);
  assert.match(modelSource, /getMyQuota/);
  assert.match(modelSource, /listMyJobs/);
  assert.match(pageSource, /当前登录状态已失效/);
  assert.match(pageSource, /to=\"\/\"/);
  assert.match(pageSource, /返回首页/);
});
