import { Navigate } from "react-router-dom";

export function DevToolsPage() {
  return <Navigate to="/system?tab=devtools" replace />;
}