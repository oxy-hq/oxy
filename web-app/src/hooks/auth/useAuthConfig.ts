import { useQuery } from "@tanstack/react-query";
import { AuthService } from "@/services/api";

export default function useAuthConfig(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true
) {
  return useQuery({
    queryKey: ["authConfig"],
    queryFn: () => AuthService.getAuthConfig(),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}
