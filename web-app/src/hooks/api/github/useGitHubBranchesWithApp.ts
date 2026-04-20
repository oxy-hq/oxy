import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubBranch } from "@/types/github";

export const useGitHubBranchesWithApp = (
  orgId: string,
  gitNamespaceId: string,
  repoName: string
) => {
  return useQuery<GitHubBranch[]>({
    queryKey: ["github", "branches", orgId, gitNamespaceId, repoName],
    queryFn: () => GitHubApiService.listBranches(orgId, gitNamespaceId, repoName),
    enabled:
      !!orgId &&
      !!gitNamespaceId &&
      gitNamespaceId.trim().length > 0 &&
      !!repoName &&
      repoName.trim().length > 0,
    staleTime: 5 * 60 * 1000
  });
};
