import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App delegates knowledge status loading and refresh polling to useKnowledgeStatusFlow", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /useKnowledgeStatusFlow/);
  assert.doesNotMatch(appSource, /loadKnowledgeStatus/);
  assert.doesNotMatch(appSource, /getRefreshKnowledgeTask/);
  assert.doesNotMatch(appSource, /startRefreshKnowledgeTask/);
  assert.doesNotMatch(appSource, /knowledgeRefreshTaskId/);
});

test("useKnowledgeStatusFlow owns knowledge status loading and refresh polling", () => {
  const source = readSource("../src/features/workspace/useKnowledgeStatusFlow.ts");

  assert.match(source, /loadKnowledgeStatus/);
  assert.match(source, /getRefreshKnowledgeTask/);
  assert.match(source, /startRefreshKnowledgeTask/);
  assert.match(source, /knowledgeRefreshTaskId/);
  assert.match(source, /handleRefreshKnowledge/);
});
