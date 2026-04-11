import { requestJson } from "./http.ts";
import type { SessionUser } from "../session/types.ts";

export interface AuthSessionResponse {
  authenticated: boolean;
  user: SessionUser | null;
}

export interface RegisterRequest {
  username: string;
  email: string;
  password: string;
}

export interface LoginRequest {
  login: string;
  password: string;
}

export interface RegisterResponse {
  user: SessionUser;
  verification_code: string;
}

export interface LoginResponse {
  user: SessionUser;
}

export interface VerifyEmailResponse {
  user: SessionUser;
}

export interface ForgotPasswordResponse {
  ok: boolean;
}

export interface ResendVerificationResponse {
  verification_code: string;
}

export function getAuthSession(): Promise<AuthSessionResponse> {
  return requestJson<AuthSessionResponse>("/api/auth/me", { backend: "web" });
}

export function registerWithPassword(body: RegisterRequest): Promise<RegisterResponse> {
  return requestJson<RegisterResponse>("/api/auth/register", {
    backend: "web",
    method: "POST",
    body,
  });
}

export function loginWithPassword(body: LoginRequest): Promise<LoginResponse> {
  return requestJson<LoginResponse>("/api/auth/login", {
    backend: "web",
    method: "POST",
    body,
  });
}

export function logoutSession(): Promise<{ ok: boolean }> {
  return requestJson<{ ok: boolean }>("/api/auth/logout", {
    backend: "web",
    method: "POST",
  });
}

export function verifyEmailCode(code: string): Promise<VerifyEmailResponse> {
  return requestJson<VerifyEmailResponse>("/api/auth/verify-email", {
    backend: "web",
    method: "POST",
    body: { code },
  });
}

export function resendVerification(body: { login?: string; password?: string }): Promise<ResendVerificationResponse> {
  return requestJson<ResendVerificationResponse>("/api/auth/resend-verification", {
    backend: "web",
    method: "POST",
    body,
  });
}

export function requestPasswordReset(login: string): Promise<ForgotPasswordResponse> {
  return requestJson<ForgotPasswordResponse>("/api/auth/forgot-password", {
    backend: "web",
    method: "POST",
    body: { login },
  });
}

export function resetPasswordWithCode(code: string, password: string): Promise<LoginResponse> {
  return requestJson<LoginResponse>("/api/auth/reset-password", {
    backend: "web",
    method: "POST",
    body: { code, password },
  });
}
