import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { getOltpErd } from "@/services/api/oltp";
import queryKeys from "../queryKey";

const useOltpErd = (workspaceId: string | undefined, enabled = true) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.oltp.erd(effectiveWorkspaceId),
    queryFn: () => getOltpErd(effectiveWorkspaceId),
    enabled,
    retry: false,
    staleTime: 5 * 60 * 1000
  });
};

export default useOltpErd;
