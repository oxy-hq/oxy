import { useMutation } from "@tanstack/react-query";
import { apiService } from "@/services/apiService";
import { ValidateEmailRequest } from "@/types/auth";

const useEmailVerification = () => {
  return useMutation({
    mutationFn: (request: ValidateEmailRequest) =>
      apiService.validateEmail(request),
  });
};

export default useEmailVerification;
