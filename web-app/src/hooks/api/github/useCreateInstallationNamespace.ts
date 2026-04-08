import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubNamespace } from "@/types/github";

export const useCreateInstallationNamespace = () => {
  const queryClient = useQueryClient();

  return useMutation<GitHubNamespace, Error, number>({
    mutationFn: (installationId: number) =>
      GitHubApiService.createInstallationNamespace(installationId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["github", "namespaces"] });
    }
  });
};
