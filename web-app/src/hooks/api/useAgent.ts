import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useAgent(
  pathb64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.agent.get(pathb64),
    queryFn: () => service.getAgent(pathb64),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
