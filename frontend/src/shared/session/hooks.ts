import { useContext } from "react";
import { SessionContext } from "./store.tsx";

export function useSession() {
  const context = useContext(SessionContext);
  if (context === null) {
    throw new Error("SessionProvider is required");
  }
  return {
    ...context,
    isLoading: context.state.status === "loading",
    isAuthenticated: context.state.status === "authenticated",
    currentUser: context.state.user,
  };
}
