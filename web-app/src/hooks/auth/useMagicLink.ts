import { useMutation } from "@tanstack/react-query";
import { useAuth } from "@/contexts/AuthContext";
import { AuthService } from "@/services/api";
import type {
  AuthResponse,
  MagicLinkRequest,
  MagicLinkVerifyRequest,
  MessageResponse
} from "@/types/auth";

export const useRequestMagicLink = () => {
  return useMutation<MessageResponse, Error, MagicLinkRequest>({
    mutationFn: AuthService.requestMagicLink
  });
};

export const useVerifyMagicLink = () => {
  const { login } = useAuth();

  return useMutation<AuthResponse, Error, MagicLinkVerifyRequest>({
    mutationFn: AuthService.verifyMagicLink,
    onSuccess: (data) => {
      login(data.token, data.user);
    }
  });
};
