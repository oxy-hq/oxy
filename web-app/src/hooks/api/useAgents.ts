import { useQuery } from "@tanstack/react-query";

import { apiClient } from "@/services/axios";

import queryKeys from "./queryKey";

export default function useAgents(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.agent.list(),
    queryFn: async () => {
      const response = await apiClient.get("/agents");
      return response.data.agents;
    },
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
