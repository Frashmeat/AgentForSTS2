export interface SessionUser {
  user_id: number;
  username: string;
  email: string;
  email_verified: boolean;
  created_at: string;
  email_verified_at?: string | null;
}

export type SessionStatus = "loading" | "anonymous" | "authenticated";

export interface SessionState {
  status: SessionStatus;
  user: SessionUser | null;
}

export interface SessionSnapshot {
  authenticated: boolean;
  user: SessionUser | null;
}

export type SessionAction =
  | { type: "loading" }
  | { type: "resolved"; snapshot: SessionSnapshot }
  | { type: "signed_in"; user: SessionUser }
  | { type: "signed_out" };
