import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { useAuth } from "@/contexts/AuthContext";
import { AuthService } from "@/services/api";
import type {
  AuthResponse,
  MagicLinkRequest,
  MagicLinkVerifyRequest,
  MessageResponse
} from "@/types/auth";
import { handlePostLoginOrgs, resolveReturnTo } from "./postLoginRedirect";

export const useRequestMagicLink = () => {
  return useMutation<MessageResponse, Error, MagicLinkRequest>({
    mutationFn: AuthService.requestMagicLink
  });
};

/**
 * Verify a magic link and log the user in.
 *
 * If `returnTo` is provided, the hook validates it via the server's
 * `/auth/return-to/validate` endpoint after a successful login and
 * navigates to it via `window.location.href` (cross-origin redirect into
 * a different `*.oxygen-hq.com` subdomain or registered external app host).
 * If validation fails or no `returnTo` is supplied, falls back to the
 * normal post-login navigation in `handlePostLoginOrgs`.
 */
export const useVerifyMagicLink = (returnTo?: string) => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, MagicLinkVerifyRequest>({
    mutationFn: AuthService.verifyMagicLink,
    onSuccess: async (data) => {
      login(data.token, data.user);

      const resolved = await resolveReturnTo(returnTo);
      if (resolved) {
        window.location.href = resolved;
        return;
      }

      const destination = handlePostLoginOrgs(data.user, data.orgs);
      navigate(destination, { replace: true });
    }
  });
};
