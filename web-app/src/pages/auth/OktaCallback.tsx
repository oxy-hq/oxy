import { useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useOktaAuth, validateOktaState } from "@/hooks/auth/useOktaAuth";
import { LoaderCircle } from "lucide-react";
import ROUTES from "@/libs/utils/routes";

const OktaCallback = () => {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const oktaAuthMutation = useOktaAuth();
  const authAttempted = useRef(false);

  useEffect(() => {
    if (authAttempted.current) return;
    authAttempted.current = true;

    const code = searchParams.get("code");
    const state = searchParams.get("state");
    const error = searchParams.get("error");
    const errorDescription = searchParams.get("error_description");

    // Check for OAuth errors first
    if (error) {
      console.error("Okta OAuth error:", error, errorDescription);
      navigate(`${ROUTES.AUTH.LOGIN}?error=oauth_failed`);
      return;
    }

    // Validate CSRF state token (critical security check)
    if (!validateOktaState(state)) {
      console.error("CSRF validation failed - potential attack detected");
      navigate(`${ROUTES.AUTH.LOGIN}?error=csrf_validation_failed`);
      return;
    }

    // Proceed with authentication if we have a valid code
    if (code) {
      oktaAuthMutation.mutate({ code });
    } else {
      navigate(`${ROUTES.AUTH.LOGIN}?error=no_code`);
    }
  }, [searchParams, navigate, oktaAuthMutation]);

  return (
    <div className="flex flex-col gap-5 items-center justify-center h-full w-full">
      <LoaderCircle className="animate-spin" />
      <p>Completing Okta authentication...</p>
    </div>
  );
};

export default OktaCallback;
