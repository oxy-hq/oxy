import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ApiKeyService } from "@/services/api/apiKey";
import type { ApiKeyListResponse } from "@/types/apiKey";
import queryKeys from "../queryKey";

const useApiKeys = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<ApiKeyListResponse, Error>({
    queryKey: queryKeys.apiKey.list(projectId),
    queryFn: () => ApiKeyService.listApiKeys(projectId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount
  });
};

export default useApiKeys;
