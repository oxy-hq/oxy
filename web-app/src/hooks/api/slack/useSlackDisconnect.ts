import { useMutation, useQueryClient } from "@tanstack/react-query";
import { SlackService } from "@/services/api/slack";
import queryKeys from "../queryKey";

export const useSlackDisconnect = (orgId: string) => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => SlackService.disconnect(orgId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.slack.installation(orgId) });
    }
  });
};
