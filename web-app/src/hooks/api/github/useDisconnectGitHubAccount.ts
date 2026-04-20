import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { GitHubApiService } from "@/services/api";

export const useDisconnectGitHubAccount = () => {
  const qc = useQueryClient();
  return useMutation<void, Error, void>({
    mutationFn: () => GitHubApiService.deleteAccount(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.github.account });
      qc.invalidateQueries({ queryKey: queryKeys.github.userInstallations });
    }
  });
};
