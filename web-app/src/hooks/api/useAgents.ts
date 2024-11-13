import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { apiClient } from "@/services/axios";

export default function useAgents(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) {
  return useQuery({
    queryKey: queryKeys.agent.list(),
    queryFn: async () => {
      const response = await apiClient.get("/agents");
      return response.data.agents;
    },
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}

