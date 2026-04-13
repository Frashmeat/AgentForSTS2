import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("App delegates single asset workspace ownership through WorkspaceHome", () => {
  const appSource = readSource("../src/App.tsx");
  const workspaceHomeSource = readSource("../src/features/workspace/WorkspaceHome.tsx");

  assert.match(appSource, /WorkspaceHome/);
  assert.doesNotMatch(appSource, /createSingleAssetWorkflowController/);
  assert.doesNotMatch(appSource, /loadSingleAssetSnapshot/);
  assert.doesNotMatch(appSource, /useProjectCreation/);
  assert.match(workspaceHomeSource, /SingleAssetWorkspaceContainer/);
});

test("single asset workspace container owns snapshot and upload wiring", () => {
  const containerSource = readSource("../src/features/single-asset/SingleAssetWorkspaceContainer.tsx");

  assert.match(containerSource, /loadSingleAssetSnapshot/);
  assert.match(containerSource, /saveSingleAssetSnapshot/);
  assert.match(containerSource, /refreshRecoveredSingleAssetApprovals/);
  assert.match(containerSource, /uploadedImageB64/);
  assert.match(containerSource, /onRequestExecution/);
});
