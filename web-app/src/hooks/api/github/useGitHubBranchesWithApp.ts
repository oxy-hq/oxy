import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import { GitHubBranch } from "@/types/github";

/**
 * Hook to fetch GitHub branches for a repository
 */
export const useGitHubBranchesWithApp = (
  gitNamespaceId: string,
  repoName: string,
) => {
  return useQuery<GitHubBranch[]>({
    queryKey: ["github", "branches", gitNamespaceId, repoName],
    queryFn: () => GitHubApiService.listBranches(gitNamespaceId, repoName),
    enabled:
      !!gitNamespaceId &&
      gitNamespaceId.trim().length > 0 &&
      !!repoName &&
      repoName.trim().length > 0,
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
};
