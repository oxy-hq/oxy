import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import { GitHubBranch } from "@/types/github";

// Hook to fetch GitHub repositories
export const useGitHubBranches = (token: string, repo: string) => {
  return useQuery<GitHubBranch[]>({
    queryKey: ["github", "branches", token, repo],
    queryFn: () => GitHubApiService.listBranches(token, repo),
    enabled:
      !!token && token.trim().length > 0 && !!repo && repo.trim().length > 0,
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
};
