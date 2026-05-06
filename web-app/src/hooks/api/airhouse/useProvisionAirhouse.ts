import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { AirhouseService } from "@/services/api";
import queryKeys from "../queryKey";

const useProvisionAirhouse = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ tenantName }: { tenantName: string }) =>
      AirhouseService.provision(effectiveWorkspaceId, tenantName),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.airhouse.connection(effectiveWorkspaceId)
      });
    }
  });
};

export default useProvisionAirhouse;
