import { useMutation } from "@tanstack/react-query";
import { service } from "@/services/service";
import { LoginRequest, AuthResponse } from "@/types/auth";
import { useAuth } from "@/contexts/AuthContext";

export const useLogin = () => {
  const { login } = useAuth();

  return useMutation<AuthResponse, Error, LoginRequest>({
    mutationFn: service.login,
    onSuccess: (data) => {
      login(data.token, data.user);
    },
    onError: (error) => {
      console.error("Login failed:", error);
    },
  });
};
