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
  const loginPage = readSource("../src/features/auth/LoginPage.tsx");
  const registerPage = readSource("../src/features/auth/RegisterPage.tsx");
  const verifyPage = readSource("../src/features/auth/VerifyEmailPage.tsx");
  const forgotPage = readSource("../src/features/auth/ForgotPasswordPage.tsx");
  const resetPage = readSource("../src/features/auth/ResetPasswordPage.tsx");

  assert.match(loginPage, /loginWithPassword/);
  assert.match(registerPage, /registerWithPassword/);
  assert.match(verifyPage, /verifyEmailCode/);
  assert.match(verifyPage, /resendVerification/);
  assert.match(verifyPage, /password/);
  assert.match(forgotPage, /requestPasswordReset/);
  assert.match(resetPage, /resetPasswordWithCode/);
});
