import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { AgentService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useAgent(
  pathb64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.agent.get(pathb64, project.id, branchName),
    queryFn: () => AgentService.getAgent(project.id, branchName, pathb64),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
