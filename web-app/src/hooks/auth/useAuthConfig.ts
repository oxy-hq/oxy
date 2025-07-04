import { AuthService } from "@/services/api";
import { useQuery } from "@tanstack/react-query";

export default function useAuthConfig(
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = true,
) {
  return useQuery({
    queryKey: ["authConfig"],
    queryFn: () => AuthService.getAuthConfig(),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
