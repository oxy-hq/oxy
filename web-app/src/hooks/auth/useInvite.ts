import { useMutation } from "@tanstack/react-query";
import { AuthService } from "@/services/api";
import type { InviteRequest, MessageResponse } from "@/types/auth";

export const useInvite = () => {
  return useMutation<MessageResponse, Error, InviteRequest>({
    mutationFn: AuthService.inviteUser
  });
};
