import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import { GitHubNamespace } from "@/types/github";

/**
 * Hook to fetch GitHub Git Namespaces (installed GitHub Apps)
 */
export const useGitHubNamespaces = () => {
  return useQuery<GitHubNamespace[]>({
    queryKey: ["github", "namespaces"],
    queryFn: () => GitHubApiService.listGitNamespaces(),
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
};
