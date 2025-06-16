import { useMutation } from "@tanstack/react-query";
import { service } from "@/services/service";
import { RegisterRequest, MessageResponse } from "@/types/auth";

export const useRegister = () => {
  return useMutation<MessageResponse, Error, RegisterRequest>({
    mutationFn: service.register,
    onSuccess: (data) => {
      console.log("Registration successful:", data.message);
    },
    onError: (error) => {
      console.error("Registration failed:", error);
    },
  });
};
