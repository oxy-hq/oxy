import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { AirhouseService } from "@/services/api";
import queryKeys from "../queryKey";

const useRevealAirhouseCredentials = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => AirhouseService.revealCredentials(effectiveWorkspaceId),
    onSuccess: () => {
      // After reveal, password_not_yet_shown flips to false; refresh the
      // cached connection info so the page reflects that immediately.
      queryClient.invalidateQueries({
        queryKey: queryKeys.airhouse.connection(effectiveWorkspaceId)
      });
    }
  });
};

export default useRevealAirhouseCredentials;
