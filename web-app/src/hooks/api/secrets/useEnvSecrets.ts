import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { SecretService } from "@/services/secretService";
import type { EnvSecret } from "@/types/secret";
import queryKeys from "../queryKey";

const useEnvSecrets = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<EnvSecret[], Error>({
    queryKey: queryKeys.secret.envList(projectId),
    queryFn: () => SecretService.listEnvSecrets(projectId)
  });
};

export default useEnvSecrets;
