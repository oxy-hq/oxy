import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import ROUTES from "@/libs/utils/routes";
import { AuthService } from "@/services/api";
import type { AuthResponse, GoogleAuthRequest } from "@/types/auth";
import { handlePostLoginOrgs } from "./postLoginRedirect";

const GOOGLE_REDIRECT_URI = `${window.location.origin}/auth/google/callback`;
const GOOGLE_STATE_KEY = "google_oauth_state";

export const useGoogleAuth = () => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, GoogleAuthRequest>({
    mutationFn: AuthService.googleAuth,
    onSuccess: (data) => {
      // Clear state after successful authentication
      sessionStorage.removeItem(GOOGLE_STATE_KEY);
      login(data.token, data.user);
      const destination = handlePostLoginOrgs(data.user, data.orgs);
      navigate(destination);
    },
    onError: (error) => {
      console.error("Google auth failed:", error);
      // Clear state on error
      sessionStorage.removeItem(GOOGLE_STATE_KEY);
      navigate(ROUTES.AUTH.LOGIN);
    }
  });
};

export const initiateGoogleAuth = async (client_id: string) => {
  // CSRF defense: the backend mints a signed, short-lived JWT. We echo it
  // through Google's `state` round-trip; the backend re-verifies signature
  // + purpose claim when we send it back with the code.
  const { state } = await AuthService.issueOAuthState();
  sessionStorage.setItem(GOOGLE_STATE_KEY, state);

  const url = new URL("https://accounts.google.com/o/oauth2/v2/auth");
  url.searchParams.set("client_id", client_id);
  url.searchParams.set("redirect_uri", GOOGLE_REDIRECT_URI);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("scope", "openid email profile");
  url.searchParams.set("access_type", "offline");
  url.searchParams.set("state", state);

  window.location.href = url.toString();
};

/**
 * Validates the OAuth state parameter to prevent CSRF attacks
 * @param receivedState - The state parameter received in the callback
 * @returns true if state is valid, false otherwise
 */
export const validateGoogleState = (receivedState: string | null): boolean => {
  if (!receivedState) {
    console.error("CSRF validation failed: No state parameter received");
    return false;
  }

  const storedState = sessionStorage.getItem(GOOGLE_STATE_KEY);
  if (!storedState) {
    console.error("CSRF validation failed: No stored state found");
    return false;
  }

  if (receivedState !== storedState) {
    console.error("CSRF validation failed: State mismatch");
    return false;
  }

  return true;
};
