import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { AirhouseService } from "@/services/api";
import queryKeys from "../queryKey";

const useRotateAirhousePassword = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => AirhouseService.rotatePassword(effectiveWorkspaceId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.airhouse.connection(effectiveWorkspaceId)
      });
    }
  });
};

export default useRotateAirhousePassword;
