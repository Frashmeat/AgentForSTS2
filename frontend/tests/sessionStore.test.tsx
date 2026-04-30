import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { createInitialSessionState, resolveSessionState, sessionReducer } from "../src/shared/session/store.tsx";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("session reducer resolves authenticated and anonymous states", () => {
  const initial = createInitialSessionState();
  const authenticated = resolveSessionState({
    authenticated: true,
    user: {
      user_id: 7,
      username: "luna",
      email: "luna@example.com",
      email_verified: true,
      created_at: "2026-04-03T10:00:00+00:00",
    },
  });
  const signedOut = sessionReducer(authenticated, { type: "signed_out" });

  assert.equal(initial.status, "loading");
  assert.equal(authenticated.status, "authenticated");
  assert.equal(authenticated.user?.username, "luna");
  assert.equal(signedOut.status, "anonymous");
  assert.equal(signedOut.user, null);
});

test("session reducer exposes unavailable state when auth backend cannot be reached", () => {
  const unavailable = sessionReducer(createInitialSessionState(), { type: "unavailable" });

  assert.equal(unavailable.status, "unavailable");
  assert.equal(unavailable.user, null);
});

test("main entry mounts browser router and session provider", () => {
  const source = readSource("../src/main.tsx");

  assert.match(source, /BrowserRouter/);
  assert.match(source, /SessionProvider/);
  assert.match(source, /<BrowserRouter>/);
});
