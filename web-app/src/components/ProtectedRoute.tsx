import { Navigate, useLocation } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import ROUTES from "@/libs/utils/routes";

interface ProtectedRouteProps {
  children: React.ReactNode;
}

const ProtectedRoute: React.FC<ProtectedRouteProps> = ({ children }) => {
  const { isAuthenticated, authConfig } = useAuth();
  const location = useLocation();

  // If authentication is not enabled, allow access without checking auth
  if (!authConfig.is_built_in_mode || !authConfig.auth_enabled) {
    return <>{children}</>;
  }

  // Authentication is enabled - check if user is authenticated
  if (!isAuthenticated()) {
    return <Navigate to={ROUTES.AUTH.LOGIN} state={{ from: location }} replace />;
  }

  return <>{children}</>;
};

export default ProtectedRoute;
