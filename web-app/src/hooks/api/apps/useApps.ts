import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { AppService } from "@/services/api";

export default function useApps(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  return useQuery({
    queryKey: queryKeys.app.list(),
    queryFn: AppService.listApps,
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
