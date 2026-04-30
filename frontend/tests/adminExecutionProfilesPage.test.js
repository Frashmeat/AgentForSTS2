import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin execution profiles page exposes CRUD actions", () => {
  const source = readSource("../src/pages/admin/AdminExecutionProfilesPage.tsx");

  assert.match(source, /createAdminExecutionProfile/);
  assert.match(source, /updateAdminExecutionProfile/);
  assert.match(source, /enableAdminExecutionProfile/);
  assert.match(source, /disableAdminExecutionProfile/);
  assert.match(source, /deleteAdminExecutionProfile/);
  assert.match(source, /新增执行配置/);
  assert.match(source, /保存执行配置/);
  assert.match(source, /执行配置已删除/);
});
