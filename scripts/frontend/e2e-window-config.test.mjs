import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

async function readJson(relativePath) {
  return JSON.parse(await readFile(path.join(repoRoot, relativePath), "utf8"));
}

test("the desktop E2E window stays in the background without changing production", async () => {
  const [e2eConfig, productionConfig] = await Promise.all([
    readJson("src-tauri/tauri.e2e.conf.json"),
    readJson("src-tauri/tauri.conf.json"),
  ]);
  const [e2eWindow] = e2eConfig.app.windows;
  const [productionWindow] = productionConfig.app.windows;

  assert.equal(e2eWindow.visible, false);
  assert.equal(e2eWindow.focus, false);
  assert.equal(e2eWindow.skipTaskbar, true);

  assert.notEqual(productionWindow.visible, false);
  assert.notEqual(productionWindow.focus, false);
  assert.notEqual(productionWindow.skipTaskbar, true);
});
