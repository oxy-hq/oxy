import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { AirhouseService } from "@/services/api";
import queryKeys from "../queryKey";

const useAirhouseConnection = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.airhouse.connection(effectiveWorkspaceId),
    queryFn: () => AirhouseService.getConnection(effectiveWorkspaceId),
    retry: false,
    staleTime: 5 * 60 * 1000
  });
};

export default useAirhouseConnection;
