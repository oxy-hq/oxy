import { useQuery } from "@tanstack/react-query";
import { SecretService } from "@/services/secretService";
import { Secret } from "@/types/secret";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useSecret = (
  id: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<Secret, Error>({
    queryKey: queryKeys.secret.item(projectId, id),
    queryFn: () => SecretService.getSecret(projectId, id),
    enabled: enabled && !!id,
    refetchOnWindowFocus,
    refetchOnMount,
  });
};

export default useSecret;
