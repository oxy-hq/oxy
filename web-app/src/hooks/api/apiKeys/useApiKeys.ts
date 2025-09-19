import { useQuery } from "@tanstack/react-query";
import { ApiKeyService } from "@/services/api/apiKey";
import { ApiKeyListResponse } from "@/types/apiKey";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useApiKeys = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<ApiKeyListResponse, Error>({
    queryKey: queryKeys.apiKey.list(projectId),
    queryFn: () => ApiKeyService.listApiKeys(projectId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });
};

export default useApiKeys;
