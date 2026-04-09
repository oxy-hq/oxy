import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { GitHubApiService } from "@/services/api";
import type { OAuthConnectResponse } from "@/types/github";

export const useConnectNamespaceFromOAuth = () => {
  const queryClient = useQueryClient();
  return useMutation<OAuthConnectResponse, Error, { code: string; state: string }>({
    mutationFn: ({ code, state }) => GitHubApiService.connectNamespaceFromOAuth(code, state),
    onSuccess: (data) => {
      // Backend stores the GitHub OAuth token on the user record for every outcome,
      // so invalidate my-installations unconditionally to reflect the new token.
      queryClient.invalidateQueries({ queryKey: queryKeys.github.myInstallations });
      if (data.status === "connected") {
        queryClient.invalidateQueries({ queryKey: queryKeys.github.namespaces });
      }
    }
  });
};
