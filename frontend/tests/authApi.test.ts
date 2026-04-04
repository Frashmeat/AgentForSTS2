import test from "node:test";
import assert from "node:assert/strict";

import {
  getAuthSession,
  loginWithPassword,
  registerWithPassword,
  requestPasswordReset,
  resetPasswordWithCode,
  verifyEmailCode,
} from "../src/shared/api/index.ts";

interface MockResponseInit {
  ok: boolean;
  body?: unknown;
  text?: string;
}

function createMockResponse(init: MockResponseInit) {
  return {
    ok: init.ok,
    async json() {
      return init.body;
    },
    async text() {
      return init.text ?? JSON.stringify(init.body ?? {});
    },
  };
}

test("auth api uses web backend auth routes", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { authenticated: false, user: null },
      });
    },
  });

  await getAuthSession();
  await registerWithPassword({ username: "luna", email: "luna@example.com", password: "secret-123" });
  await loginWithPassword({ login: "luna", password: "secret-123" });
  await verifyEmailCode("verify-1");
  await requestPasswordReset("luna@example.com");
  await resetPasswordWithCode("reset-1", "secret-456");

  assert.equal(calls[0].input, "/api/auth/me");
  assert.equal(calls[1].input, "/api/auth/register");
  assert.equal(calls[2].input, "/api/auth/login");
  assert.equal(calls[3].input, "/api/auth/verify-email");
  assert.equal(calls[4].input, "/api/auth/forgot-password");
  assert.equal(calls[5].input, "/api/auth/reset-password");
  assert.equal(calls[1].init?.method, "POST");
});

test("auth api respects configured web backend base", async () => {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  Object.assign(globalThis, {
    __AGENT_THE_SPIRE_API_BASES__: {
      web: "http://127.0.0.1:7870",
    },
    fetch: async (input: unknown, init?: RequestInit) => {
      calls.push({ input, init });
      return createMockResponse({
        ok: true,
        body: { authenticated: false, user: null },
      });
    },
  });

  await getAuthSession();

  assert.equal(calls[0].input, "http://127.0.0.1:7870/api/auth/me");
});
