import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import ROUTES from "@/libs/utils/routes";
import { AuthService } from "@/services/api";
import type { AuthResponse, OktaAuthRequest } from "@/types/auth";

const OKTA_REDIRECT_URI = `${window.location.origin}/auth/okta/callback`;
const OKTA_STATE_KEY = "okta_oauth_state";

export const useOktaAuth = () => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, OktaAuthRequest>({
    mutationFn: AuthService.oktaAuth,
    onSuccess: (data) => {
      // Clear state after successful authentication
      sessionStorage.removeItem(OKTA_STATE_KEY);
      login(data.token, data.user);
      navigate(ROUTES.ROOT);
    },
    onError: (error) => {
      console.error("Okta auth failed:", error);
      // Clear state on error
      sessionStorage.removeItem(OKTA_STATE_KEY);
      navigate(ROUTES.AUTH.LOGIN);
    }
  });
};

export const initiateOktaAuth = (client_id: string, domain: string) => {
  const url = new URL(`https://${domain}/oauth2/v1/authorize`);
  url.searchParams.set("client_id", client_id);
  url.searchParams.set("redirect_uri", OKTA_REDIRECT_URI);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("scope", "openid email profile");

  // Generate cryptographically secure random state for CSRF protection
  const state = crypto.randomUUID ? crypto.randomUUID() : generateSecureRandomState();

  // Store state in sessionStorage for validation on callback
  sessionStorage.setItem(OKTA_STATE_KEY, state);
  url.searchParams.set("state", state);

  window.location.href = url.toString();
};

/**
 * Generates a cryptographically secure random state token
 * Fallback for browsers that don't support crypto.randomUUID
 */
const generateSecureRandomState = (): string => {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  return Array.from(array, (byte) => byte.toString(16).padStart(2, "0")).join("");
};

/**
 * Validates the OAuth state parameter to prevent CSRF attacks
 * @param receivedState - The state parameter received in the callback
 * @returns true if state is valid, false otherwise
 */
export const validateOktaState = (receivedState: string | null): boolean => {
  if (!receivedState) {
    console.error("CSRF validation failed: No state parameter received");
    return false;
  }

  const storedState = sessionStorage.getItem(OKTA_STATE_KEY);
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
