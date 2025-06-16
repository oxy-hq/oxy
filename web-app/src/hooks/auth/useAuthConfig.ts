import { service } from "@/services/service";
import { useQuery } from "@tanstack/react-query";

export default function useAuthConfig(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  return useQuery({
    queryKey: ["authConfig"],
    queryFn: () => service.getAuthConfig(),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
