import { useMutation } from "@tanstack/react-query";
import { AuthService } from "@/services/api";
import { ValidateEmailRequest } from "@/types/auth";

const useEmailVerification = () => {
  return useMutation({
    mutationFn: (request: ValidateEmailRequest) =>
      AuthService.validateEmail(request),
  });
};

export default useEmailVerification;
