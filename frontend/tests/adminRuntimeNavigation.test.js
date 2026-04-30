import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("admin runtime and audit routes are split under admin layout", () => {
  const appSource = readSource("../src/App.tsx");
  const layoutSource = readSource("../src/pages/admin/AdminLayout.tsx");
  const auditSource = readSource("../src/pages/admin/AdminAuditPage.tsx");
  const runtimeSource = readSource("../src/pages/admin/AdminRuntimePage.tsx");
  const knowledgePacksSource = readSource("../src/pages/admin/AdminKnowledgePacksPage.tsx");

  assert.match(appSource, /AdminRuntimePage/);
  assert.match(appSource, /AdminAuditPage/);
  assert.match(appSource, /AdminKnowledgePacksPage/);
  assert.match(appSource, /path="runtime"/);
  assert.match(appSource, /path="audit"/);
  assert.match(appSource, /path="knowledge-packs"/);
  assert.match(appSource, /to="\/admin\/audit"/);
  assert.match(layoutSource, /\/admin\/runtime/);
  assert.match(layoutSource, /\/admin\/audit/);
  assert.match(layoutSource, /\/admin\/knowledge-packs/);
  assert.match(runtimeSource, /loadPlatformQueueWorkerStatus/);
  assert.match(runtimeSource, /getAdminWorkstationRuntimeStatus/);
  assert.match(runtimeSource, /getAdminWorkstationRuntimeLogs/);
  assert.match(runtimeSource, /Web Workstation 日志/);
  assert.match(runtimeSource, /服务器生成能力/);
  assert.match(runtimeSource, /服务器构建/);
  assert.match(runtimeSource, /服务器部署/);
  assert.match(auditSource, /listAdminAuditEvents/);
  assert.match(auditSource, /formatAdminEventType/);
  assert.match(knowledgePacksSource, /listAdminKnowledgePacks/);
  assert.match(knowledgePacksSource, /exportCurrentKnowledgePack/);
  assert.match(knowledgePacksSource, /uploadAdminKnowledgePack/);
  assert.match(knowledgePacksSource, /activateAdminKnowledgePack/);
  assert.match(knowledgePacksSource, /rollbackAdminKnowledgePack/);
  assert.match(knowledgePacksSource, /从本机工作站上传/);
  assert.match(knowledgePacksSource, /文件列表/);
  assert.match(knowledgePacksSource, /源码统计/);
  assert.match(knowledgePacksSource, /game_cs_count/);
  assert.match(knowledgePacksSource, /pack\?\.files/);
});
