import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { OltpService } from "@/services/api/oltp";
import queryKeys from "../queryKey";

const useOltpConnection = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.oltp.connection(effectiveWorkspaceId),
    queryFn: () => OltpService.getConnection(effectiveWorkspaceId),
    retry: false,
    staleTime: 5 * 60 * 1000
  });
};

export default useOltpConnection;
