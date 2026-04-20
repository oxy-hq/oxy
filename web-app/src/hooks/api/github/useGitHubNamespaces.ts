import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubNamespace } from "@/types/github";

export const useGitHubNamespaces = (orgId: string) => {
  return useQuery<GitHubNamespace[]>({
    queryKey: ["github", "namespaces", orgId],
    queryFn: () => GitHubApiService.listGitNamespaces(orgId),
    enabled: !!orgId,
    staleTime: 5 * 60 * 1000
  });
};
