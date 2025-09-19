import { useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useGoogleAuth } from "@/hooks/auth/useGoogleAuth";
import { LoaderCircle } from "lucide-react";
import ROUTES from "@/libs/utils/routes";

const GoogleCallback = () => {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const googleAuthMutation = useGoogleAuth();
  const authAttempted = useRef(false);

  useEffect(() => {
    if (authAttempted.current) return;
    authAttempted.current = true;

    const code = searchParams.get("code");
    const error = searchParams.get("error");

    if (error) {
      console.error("Google OAuth error:", error);
      navigate(`${ROUTES.AUTH.LOGIN}?error=oauth_failed`);
      return;
    }

    if (code) {
      googleAuthMutation.mutate({ code });
    } else {
      navigate(`${ROUTES.AUTH.LOGIN}?error=no_code`);
    }
  }, []);

  return (
    <div className="flex flex-col gap-5 items-center justify-center h-full w-full">
      <LoaderCircle className="animate-spin" />
      <p>Completing Google authentication...</p>
    </div>
  );
};

export default GoogleCallback;
