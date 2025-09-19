import { useQuery } from "@tanstack/react-query";
import { SecretService } from "@/services/secretService";
import { SecretListResponse } from "@/types/secret";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useSecrets = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<SecretListResponse, Error>({
    queryKey: queryKeys.secret.list(projectId),
    queryFn: () => {
      return SecretService.listSecrets(projectId);
    },
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });
};

export default useSecrets;
