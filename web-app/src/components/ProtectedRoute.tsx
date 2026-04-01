import { Navigate, useLocation } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import ROUTES from "@/libs/utils/routes";

interface ProtectedRouteProps {
  children: React.ReactNode;
}

const ProtectedRoute: React.FC<ProtectedRouteProps> = ({ children }) => {
  const { isAuthenticated, authConfig } = useAuth();
  const location = useLocation();

  // Defensive guard: ProtectedRoute is only rendered when auth_enabled is true
  // in App.tsx, but this check makes the component safe if used independently.
  if (!authConfig.auth_enabled) {
    return <>{children}</>;
  }

  // Authentication is enabled - check if user is authenticated
  if (!isAuthenticated()) {
    return <Navigate to={ROUTES.AUTH.LOGIN} state={{ from: location }} replace />;
  }

  return <>{children}</>;
};

export default ProtectedRoute;
