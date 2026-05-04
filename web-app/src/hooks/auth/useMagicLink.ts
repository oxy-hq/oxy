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
import { handlePostLoginOrgs } from "./postLoginRedirect";

export const useRequestMagicLink = () => {
  return useMutation<MessageResponse, Error, MagicLinkRequest>({
    mutationFn: AuthService.requestMagicLink
  });
};

export const useVerifyMagicLink = () => {
  const { login } = useAuth();
  const navigate = useNavigate();

  return useMutation<AuthResponse, Error, MagicLinkVerifyRequest>({
    mutationFn: AuthService.verifyMagicLink,
    onSuccess: (data) => {
      login(data.token, data.user);
      const destination = handlePostLoginOrgs(data.user, data.orgs);
      navigate(destination, { replace: true });
    }
  });
};
