import { Navigate } from "react-router-dom";

import useProjectPath from "@/stores/useProjectPath";

interface ProtectedRouteProps {
  children: React.ReactNode;
}

export default function ProtectedRoute({ children }: ProtectedRouteProps) {
  const { projectPath } = useProjectPath();

  if (!projectPath) {
    return <Navigate to="/init" replace />;
  }

  return <>{children}</>;
}
