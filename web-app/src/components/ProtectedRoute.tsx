import { useAuth } from "@/contexts/AuthContext";
import { Navigate, useLocation } from "react-router-dom";
import ROUTES from "@/libs/utils/routes";

interface ProtectedRouteProps {
  children: React.ReactNode;
}

const ProtectedRoute: React.FC<ProtectedRouteProps> = ({ children }) => {
  const { isAuthenticated, authConfig } = useAuth();
  const location = useLocation();

  if (!authConfig.cloud) {
    return <>{children}</>;
  }

  if (!authConfig.is_built_in_mode || !authConfig.auth_enabled) {
    return <>{children}</>;
  }

  if (!isAuthenticated()) {
    return (
      <Navigate to={ROUTES.AUTH.LOGIN} state={{ from: location }} replace />
    );
  }

  return <>{children}</>;
};

export default ProtectedRoute;
