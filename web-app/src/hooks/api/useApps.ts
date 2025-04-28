import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useApps(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.app.list(),
    queryFn: service.listApps,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
