import { useMutation } from "@tanstack/react-query";
import { AuthService } from "@/services/api";
import type { MessageResponse, RegisterRequest } from "@/types/auth";

export const useRegister = () => {
  return useMutation<MessageResponse, Error, RegisterRequest>({
    mutationFn: AuthService.register,
    onSuccess: (data) => {
      console.log("Registration successful:", data.message);
    },
    onError: (error) => {
      console.error("Registration failed:", error);
    }
  });
};
