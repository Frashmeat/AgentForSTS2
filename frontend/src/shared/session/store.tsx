import { createContext, createElement, useEffect, useReducer, type ReactNode } from "react";
import { getAuthSession } from "../api/auth.ts";
import type { SessionAction, SessionSnapshot, SessionState, SessionUser } from "./types.ts";

interface SessionContextValue {
  state: SessionState;
  refreshSession: () => Promise<void>;
  markSignedIn: (user: SessionUser) => void;
  markSignedOut: () => void;
}

export const SessionContext = createContext<SessionContextValue | null>(null);

export function createInitialSessionState(): SessionState {
  return {
    status: "loading",
    user: null,
  };
}

export function resolveSessionState(snapshot: SessionSnapshot): SessionState {
  if (!snapshot.authenticated || snapshot.user === null) {
    return {
      status: "anonymous",
      user: null,
    };
  }
  return {
    status: "authenticated",
    user: snapshot.user,
  };
}

export function sessionReducer(state: SessionState, action: SessionAction): SessionState {
  switch (action.type) {
    case "loading":
      return {
        ...state,
        status: "loading",
      };
    case "resolved":
      return resolveSessionState(action.snapshot);
    case "unavailable":
      return {
        status: "unavailable",
        user: null,
      };
    case "signed_in":
      return {
        status: "authenticated",
        user: action.user,
      };
    case "signed_out":
      return {
        status: "anonymous",
        user: null,
      };
    default:
      return state;
  }
}

export function SessionProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(sessionReducer, undefined, createInitialSessionState);

  async function refreshSession() {
    dispatch({ type: "loading" });
    try {
      const snapshot = await getAuthSession();
      dispatch({ type: "resolved", snapshot });
    } catch {
      dispatch({ type: "unavailable" });
    }
  }

  useEffect(() => {
    void refreshSession();
  }, []);

  return createElement(
    SessionContext.Provider,
    {
      value: {
        state,
        refreshSession,
        markSignedIn(user: SessionUser) {
          dispatch({ type: "signed_in", user });
        },
        markSignedOut() {
          dispatch({ type: "signed_out" });
        },
      },
    },
    children,
  );
}
