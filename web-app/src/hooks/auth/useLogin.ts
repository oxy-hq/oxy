import { useMutation } from "@tanstack/react-query";
import { useAuth } from "@/contexts/AuthContext";
import { AuthService } from "@/services/api";
import type { AuthResponse, LoginRequest } from "@/types/auth";

export const useLogin = () => {
  const { login } = useAuth();

  return useMutation<AuthResponse, Error, LoginRequest>({
    mutationFn: AuthService.login,
    onSuccess: (data) => {
      login(data.token, data.user);
    },
    onError: (error) => {
      console.error("Login failed:", error);
    }
  });
};
