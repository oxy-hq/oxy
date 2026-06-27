import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { authOrigin } from "@/libs/orgSubdomain";
import ROUTES from "@/libs/utils/routes";
import { AuthService } from "@/services/api";
import type { AuthResponse, GoogleAuthRequest } from "@/types/auth";
import {
  consumeReturnTo,
  handlePostLoginOrgs,
  resolveReturnTo,
  returnToFromUrl,
  stashReturnTo
} from "./postLoginRedirect";

// On an org subdomain `authOrigin()` is the centralized app host, so the
// provider's redirect_uri stays the one registered host (never the subdomain).
const GOOGLE_REDIRECT_URI = `${authOrigin()}/auth/google/callback`;
const GOOGLE_STATE_KEY = "google_oauth_state";

export const useGoogleAuth = () => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, GoogleAuthRequest>({
    mutationFn: AuthService.googleAuth,
    onSuccess: async (data) => {
      // Clear state after successful authentication
      sessionStorage.removeItem(GOOGLE_STATE_KEY);
      login(data.token, data.user);
      // Honor a stashed `return_to` (e.g. a custom-app subdomain the user was
      // bounced from) before the default org/workspace destination.
      const returnTo = await resolveReturnTo(consumeReturnTo());
      if (returnTo) {
        window.location.href = returnTo;
        return;
      }
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
  // Carry the login page's `return_to` across the provider round-trip so the
  // callback can send the user back where they came from.
  stashReturnTo(returnToFromUrl());

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
