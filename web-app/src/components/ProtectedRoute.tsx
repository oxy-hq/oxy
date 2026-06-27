import { Navigate, useLocation } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { isOnOrgSubdomain, redirectToCentralLogin } from "@/libs/orgSubdomain";
import ROUTES from "@/libs/utils/routes";

interface ProtectedRouteProps {
  children: React.ReactNode;
}

const ProtectedRoute: React.FC<ProtectedRouteProps> = ({ children }) => {
  const { isAuthenticated, authConfig } = useAuth();
  const location = useLocation();

  if (!authConfig.auth_enabled) {
    return <>{children}</>;
  }

  if (!isAuthenticated()) {
    // On an org subdomain, auth is centralized on the app host — never render a
    // local /login (its OAuth redirect_uri would be the subdomain → provider
    // rejects it). Bounce to the app-host login. OrgSubdomainAuthGate normally
    // handles this on boot; this covers mid-session token loss.
    if (isOnOrgSubdomain() && redirectToCentralLogin()) {
      return null;
    }
    return <Navigate to={ROUTES.AUTH.LOGIN} state={{ from: location }} replace />;
  }

  return <>{children}</>;
};

export default ProtectedRoute;
