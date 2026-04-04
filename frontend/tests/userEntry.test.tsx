import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("user entry exposes login register and authenticated avatar states", () => {
  const userEntry = readSource("../src/components/UserEntry.tsx");

  assert.match(userEntry, /登录/);
  assert.match(userEntry, /注册/);
  assert.match(userEntry, /renderAvatar/);
  assert.match(userEntry, /logoutSession/);
});

test("app header mounts user entry and auth routes", () => {
  const appSource = readSource("../src/App.tsx");

  assert.match(appSource, /UserEntry/);
  assert.match(appSource, /\/auth\/login/);
  assert.match(appSource, /\/auth\/register/);
  assert.match(appSource, /\/me/);
});
