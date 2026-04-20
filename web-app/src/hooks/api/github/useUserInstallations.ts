import { useQuery } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { GitHubApiService } from "@/services/api";
import type { UserInstallation } from "@/types/github";

export const useUserInstallations = (opts?: { enabled?: boolean }) =>
  useQuery<UserInstallation[], Error>({
    queryKey: queryKeys.github.userInstallations,
    queryFn: () => GitHubApiService.listUserInstallations(),
    enabled: opts?.enabled ?? true,
    retry: false
  });
