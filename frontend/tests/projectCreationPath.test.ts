import test from "node:test";
import assert from "node:assert/strict";

import { deriveCreateProjectRequest } from "../src/shared/projectCreation.ts";

test("deriveCreateProjectRequest splits full project root into target dir and name", () => {
  assert.deepEqual(
    deriveCreateProjectRequest("E:/STS2mod/MyDarkMod"),
    {
      name: "MyDarkMod",
      target_dir: "E:/STS2mod",
    },
  );
});

test("deriveCreateProjectRequest normalizes trailing slash", () => {
  assert.deepEqual(
    deriveCreateProjectRequest("E:/STS2mod/MyDarkMod/"),
    {
      name: "MyDarkMod",
      target_dir: "E:/STS2mod",
    },
  );
});

test("deriveCreateProjectRequest rejects path without parent directory", () => {
  assert.throws(
    () => deriveCreateProjectRequest("MyDarkMod"),
    /完整的项目路径/,
  );
});
