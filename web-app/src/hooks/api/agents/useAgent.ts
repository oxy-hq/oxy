import { useQuery } from "@tanstack/react-query";

import queryKeys from "../queryKey";
import { AgentService } from "@/services/api";

export default function useAgent(
  pathb64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  return useQuery({
    queryKey: queryKeys.agent.get(pathb64),
    queryFn: () => AgentService.getAgent(pathb64),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
