import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubRepository } from "@/types/github";

// Hook to fetch GitHub repositories
export const useGitHubRepositories = (token: string) => {
  return useQuery<GitHubRepository[]>({
    queryKey: ["github", "repositories", token],
    queryFn: () => GitHubApiService.listRepositories(token),
    enabled: !!token && token.trim().length > 0,
    staleTime: 5 * 60 * 1000 // 5 minutes
  });
};
