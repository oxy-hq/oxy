import { useMutation } from "@tanstack/react-query";
import { AuthService } from "@/services/api";
import { GoogleAuthRequest, AuthResponse } from "@/types/auth";
import { useAuth } from "@/contexts/AuthContext";
import { useNavigate } from "react-router-dom";
import ROUTES from "@/libs/utils/routes";

const GOOGLE_REDIRECT_URI = `${window.location.origin}/auth/google/callback`;

export const useGoogleAuth = () => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, GoogleAuthRequest>({
    mutationFn: AuthService.googleAuth,
    onSuccess: (data) => {
      login(data.token, data.user);
      navigate(ROUTES.ROOT);
    },
    onError: (error) => {
      console.error("Google auth failed:", error);
      navigate(ROUTES.AUTH.LOGIN);
    },
  });
};

export const initiateGoogleAuth = (client_id: string) => {
  const url = new URL("https://accounts.google.com/o/oauth2/v2/auth");
  url.searchParams.set("client_id", client_id);
  url.searchParams.set("redirect_uri", GOOGLE_REDIRECT_URI);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("scope", "openid email profile");
  url.searchParams.set("access_type", "offline");

  window.location.href = url.toString();
};
