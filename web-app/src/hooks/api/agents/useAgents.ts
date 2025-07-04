import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { AgentService } from "@/services/api";

export default function useAgents(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  return useQuery({
    queryKey: queryKeys.agent.list(),
    queryFn: AgentService.listAgents,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
