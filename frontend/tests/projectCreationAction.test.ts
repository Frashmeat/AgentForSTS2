import test from "node:test";
import assert from "node:assert/strict";

import { createProjectFromRoot } from "../src/shared/projectCreation.ts";

test("createProjectFromRoot derives request and returns created path", async () => {
  const calls: Array<{ name: string; target_dir: string }> = [];

  const result = await createProjectFromRoot("E:/STS2mod/MyMod", async (request) => {
    calls.push(request);
    return { project_path: "E:/STS2mod/MyMod" };
  });

  assert.deepEqual(calls, [{ name: "MyMod", target_dir: "E:/STS2mod" }]);
  assert.equal(result.project_path, "E:/STS2mod/MyMod");
});

test("createProjectFromRoot keeps path derivation errors", async () => {
  await assert.rejects(() => createProjectFromRoot("MyMod"), /完整的项目路径/);
});
