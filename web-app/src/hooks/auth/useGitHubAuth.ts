import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import ROUTES from "@/libs/utils/routes";
import { AuthService } from "@/services/api";
import type { AuthResponse, GitHubAuthRequest } from "@/types/auth";

const GITHUB_AUTH_REDIRECT_URI = `${window.location.origin}/github/callback`;
const GITHUB_STATE_KEY = "github_oauth_login_state";

export const useGitHubAuth = () => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, GitHubAuthRequest>({
    mutationFn: AuthService.githubAuth,
    onSuccess: (data) => {
      sessionStorage.removeItem(GITHUB_STATE_KEY);
      login(data.token, data.user);
      navigate(ROUTES.ROOT);
    },
    onError: (error) => {
      console.error("GitHub auth failed:", error);
      sessionStorage.removeItem(GITHUB_STATE_KEY);
      navigate(ROUTES.AUTH.LOGIN);
    }
  });
};

export const initiateGitHubAuth = (clientId: string) => {
  const url = new URL("https://github.com/login/oauth/authorize");
  url.searchParams.set("client_id", clientId);
  url.searchParams.set("redirect_uri", GITHUB_AUTH_REDIRECT_URI);
  url.searchParams.set("scope", "read:user user:email");

  const state = crypto.randomUUID ? crypto.randomUUID() : generateSecureRandomState();
  sessionStorage.setItem(GITHUB_STATE_KEY, state);
  url.searchParams.set("state", state);

  window.location.href = url.toString();
};

export const validateGitHubAuthState = (receivedState: string | null): boolean => {
  if (!receivedState) return false;
  const storedState = sessionStorage.getItem(GITHUB_STATE_KEY);
  if (!storedState) return false;
  return receivedState === storedState;
};

const generateSecureRandomState = (): string => {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  return Array.from(array, (byte) => byte.toString(16).padStart(2, "0")).join("");
};
