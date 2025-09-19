import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { AgentService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useAgents(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.agent.list(project.id, branchName),
    queryFn: () => AgentService.listAgents(project.id, branchName),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
