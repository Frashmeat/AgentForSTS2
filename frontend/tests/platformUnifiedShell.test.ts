import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function readSource(path: string) {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("auth and user-center pages reuse a dedicated platform page shell", () => {
  const authShellSource = readSource("../src/features/auth/LoginPage.tsx");
  const registerSource = readSource("../src/features/auth/RegisterPage.tsx");
  const verifySource = readSource("../src/features/auth/VerifyEmailPage.tsx");
  const forgotSource = readSource("../src/features/auth/ForgotPasswordPage.tsx");
  const resetSource = readSource("../src/features/auth/ResetPasswordPage.tsx");
  const userCenterSource = readSource("../src/features/user-center/page.tsx");
  const jobDetailSource = readSource("../src/features/user-center/job-detail-page.tsx");

  assert.equal(existsSync(new URL("../src/components/platform/PlatformPageShell.tsx", import.meta.url)), true);
  assert.match(authShellSource, /PlatformPageShell/);
  assert.match(registerSource, /PlatformPageShell/);
  assert.match(verifySource, /PlatformPageShell/);
  assert.match(forgotSource, /PlatformPageShell/);
  assert.match(resetSource, /PlatformPageShell/);
  assert.match(userCenterSource, /PlatformPageShell/);
  assert.match(jobDetailSource, /PlatformPageShell/);
});

test("platform page shell styles reuse workspace visual language", () => {
  const cssSource = readSource("../src/index.css");

  assert.match(cssSource, /\.platform-page-shell\s*\{/);
  assert.match(cssSource, /\.platform-page-hero\s*\{/);
  assert.match(cssSource, /\.platform-page-card\s*\{/);
  assert.match(cssSource, /var\(--workspace-accent-strong\)/);
  assert.match(cssSource, /var\(--workspace-panel-border\)/);
});
