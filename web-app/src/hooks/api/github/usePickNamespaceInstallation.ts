import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubNamespace } from "@/types/github";

export const usePickNamespaceInstallation = () => {
  const queryClient = useQueryClient();
  return useMutation<GitHubNamespace, Error, { installation_id: number; selection_token: string }>({
    mutationFn: ({ installation_id, selection_token }) =>
      GitHubApiService.pickNamespaceInstallation(installation_id, selection_token),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["github", "namespaces"] });
    }
  });
};
