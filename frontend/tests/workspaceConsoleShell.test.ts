import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("workspace root is wrapped by a dedicated WorkspaceShell", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /import\s+\{\s*WorkspaceShell\s*\}\s+from\s+"\.\/components\/workspace\/WorkspaceShell\.tsx";/);
  assert.match(appSource, /const\s+workspaceNavItems\s*=\s*\[/);
  assert.match(appSource, /<WorkspaceShell[\s\S]*activeTab=\{activeTab\}[\s\S]*>\s*\{renderWorkspaceContent\(\)\}\s*<\/WorkspaceShell>/);
  assert.equal(existsSync(new URL("../src/components/workspace/WorkspaceShell.tsx", import.meta.url)), true);
});

test("workspace theme tokens are defined from the approved purple-blue palette", () => {
  const cssSource = readSource("../src/index.css");

  assert.match(cssSource, /--workspace-nav-bg:\s*#13132B;/);
  assert.match(cssSource, /--workspace-nav-active:\s*#1B1553;/);
  assert.match(cssSource, /--workspace-accent-strong:\s*#241953;/);
  assert.match(cssSource, /--workspace-accent:\s*#724A91;/);
  assert.match(cssSource, /--workspace-text-muted:\s*#626B96;/);
});
