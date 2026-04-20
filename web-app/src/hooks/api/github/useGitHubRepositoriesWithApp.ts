import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubRepository } from "@/types/github";

export const useGitHubRepositoriesWithApp = (orgId: string, gitNamespaceId: string) => {
  return useQuery<GitHubRepository[]>({
    queryKey: ["github", "repositories", orgId, gitNamespaceId],
    queryFn: () => GitHubApiService.listRepositories(orgId, gitNamespaceId),
    enabled: !!orgId && !!gitNamespaceId && gitNamespaceId.trim().length > 0,
    staleTime: 5 * 60 * 1000
  });
};
