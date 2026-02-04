import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";

/**
 * Hook to get the GitHub App installation URL
 */
export const useGitHubInstallAppUrl = () => {
  return useQuery<string>({
    queryKey: ["github", "install-app-url"],
    queryFn: () => GitHubApiService.getInstallAppUrl(),
    staleTime: 30 * 60 * 1000 // 30 minutes
  });
};
