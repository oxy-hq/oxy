import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { GitHubNamespace } from "@/types/github";

export const useCreatePATNamespace = () => {
  const queryClient = useQueryClient();

  return useMutation<GitHubNamespace, Error, string>({
    mutationFn: (token: string) => GitHubApiService.createPATNamespace(token),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["github", "namespaces"] });
    }
  });
};
