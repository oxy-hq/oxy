import { useQuery } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { AuthService } from "@/services/api";

export default function useAuthConfig(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true
) {
  return useQuery({
    queryKey: queryKeys.authConfig.current(),
    queryFn: () => AuthService.getAuthConfig(),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}
