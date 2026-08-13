import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const [commands, shell, studio] = await Promise.all([
  readFile(new URL("../../src-tauri/src/commands/stage2.rs", import.meta.url), "utf8"),
  readFile(new URL("../../src-tauri/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../../src/pages/CompositionStudioPage.tsx", import.meta.url), "utf8"),
]);

test("composition adjustment stays behind its dedicated backend-authored command", () => {
  assert.match(commands, /pub async fn adjust_composition_item\s*\(/);
  assert.match(
    commands,
    /if request\.execution\.is_some\(\) \|\| request\.adjustment\.is_some\(\)/,
  );
  assert.match(shell, /commands::stage2::adjust_composition_item/);
});

test("ordinary composition generation uses the internal until-passed policy", () => {
  assert.match(studio, /\{ kind: "until_passed" \}/);
  assert.doesNotMatch(studio, /kind: "max_rounds"/);
});
