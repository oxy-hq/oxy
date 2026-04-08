import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubNamespace } from "@/types/github";

export const usePickNamespaceInstallation = () => {
  const queryClient = useQueryClient();
  return useMutation<GitHubNamespace, Error, number>({
    mutationFn: (installation_id) => GitHubApiService.pickNamespaceInstallation(installation_id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["github", "namespaces"] });
    }
  });
};
