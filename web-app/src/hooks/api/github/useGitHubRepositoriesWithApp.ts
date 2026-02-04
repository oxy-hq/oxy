import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubRepository } from "@/types/github";

/**
 * Hook to fetch GitHub repositories for a git namespace
 */
export const useGitHubRepositoriesWithApp = (gitNamespaceId: string) => {
  return useQuery<GitHubRepository[]>({
    queryKey: ["github", "repositories", gitNamespaceId],
    queryFn: () => GitHubApiService.listRepositories(gitNamespaceId),
    enabled: !!gitNamespaceId && gitNamespaceId.trim().length > 0,
    staleTime: 5 * 60 * 1000 // 5 minutes
  });
};
