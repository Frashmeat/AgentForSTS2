import type { StatusNoticeItem } from "../../components/StatusNotice.tsx";

export type AuthFormStatus = "idle" | "submitting" | "success" | "error";
export type AuthStatusNoticeHandler = (notice: Omit<StatusNoticeItem, "id">) => void;

export interface AuthFormState {
  status: AuthFormStatus;
  message: string;
}

export function createIdleAuthFormState(): AuthFormState {
  return {
    status: "idle",
    message: "",
  };
}

export function createSubmittingAuthFormState(): AuthFormState {
  return {
    status: "submitting",
    message: "",
  };
}

export function createSuccessAuthFormState(message: string): AuthFormState {
  return {
    status: "success",
    message,
  };
}

export function createErrorAuthFormState(message: string): AuthFormState {
  return {
    status: "error",
    message,
  };
}
