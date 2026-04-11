import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  createErrorAuthFormState,
  createSubmittingAuthFormState,
  createSuccessAuthFormState,
} from "../src/features/auth/formModel.ts";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("auth form model exposes submitting success and error states", () => {
  assert.equal(createSubmittingAuthFormState().status, "submitting");
  assert.equal(createSuccessAuthFormState("ok").message, "ok");
  assert.equal(createErrorAuthFormState("fail").status, "error");
});

test("auth pages bind to auth api flows", () => {
  const homeLinkSource = readSource("../src/features/auth/AuthHomeLink.tsx");
  const loginPage = readSource("../src/features/auth/LoginPage.tsx");
  const registerPage = readSource("../src/features/auth/RegisterPage.tsx");
  const verifyPage = readSource("../src/features/auth/VerifyEmailPage.tsx");
  const forgotPage = readSource("../src/features/auth/ForgotPasswordPage.tsx");
  const resetPage = readSource("../src/features/auth/ResetPasswordPage.tsx");

  assert.match(homeLinkSource, /to=\"\/\"/);
  assert.match(homeLinkSource, /返回首页/);
  assert.match(loginPage, /AuthHomeLink/);
  assert.match(loginPage, /loginWithPassword/);
  assert.match(loginPage, /refreshSession/);
  assert.match(registerPage, /AuthHomeLink/);
  assert.match(registerPage, /registerWithPassword/);
  assert.doesNotMatch(registerPage, /\?code=/);
  assert.match(registerPage, /state:\s*\{\s*code:/);
  assert.match(verifyPage, /AuthHomeLink/);
  assert.match(verifyPage, /verifyEmailCode/);
  assert.match(verifyPage, /resendVerification/);
  assert.match(verifyPage, /useLocation/);
  assert.doesNotMatch(verifyPage, /useSearchParams/);
  assert.match(verifyPage, /password/);
  assert.match(forgotPage, /AuthHomeLink/);
  assert.match(forgotPage, /requestPasswordReset/);
  assert.doesNotMatch(forgotPage, /\?code=/);
  assert.match(resetPage, /AuthHomeLink/);
  assert.match(resetPage, /resetPasswordWithCode/);
  assert.match(resetPage, /refreshSession/);
  assert.match(resetPage, /useLocation/);
  assert.doesNotMatch(resetPage, /useSearchParams/);
});
