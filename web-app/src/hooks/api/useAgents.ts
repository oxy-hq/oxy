import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useAgents(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.agent.list(),
    queryFn: service.listAgents,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
