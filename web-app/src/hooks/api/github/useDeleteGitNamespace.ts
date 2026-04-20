import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";

export const useDeleteGitNamespace = () => {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { orgId: string; id: string }>({
    mutationFn: ({ orgId, id }) => GitHubApiService.deleteGitNamespace(orgId, id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["github", "namespaces"] });
    }
  });
};
