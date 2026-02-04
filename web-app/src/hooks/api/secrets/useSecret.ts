import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { SecretService } from "@/services/secretService";
import type { Secret } from "@/types/secret";
import queryKeys from "../queryKey";

const useSecret = (
  id: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<Secret, Error>({
    queryKey: queryKeys.secret.item(projectId, id),
    queryFn: () => SecretService.getSecret(projectId, id),
    enabled: enabled && !!id,
    refetchOnWindowFocus,
    refetchOnMount
  });
};

export default useSecret;
