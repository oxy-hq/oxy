import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import type { OAuthConnectResponse } from "@/types/github";

export const useConnectNamespaceFromOAuth = () => {
  const queryClient = useQueryClient();
  return useMutation<OAuthConnectResponse, Error, { code: string; state: string }>({
    mutationFn: ({ code, state }) => GitHubApiService.connectNamespaceFromOAuth(code, state),
    onSuccess: (data) => {
      if (data.status === "connected") {
        queryClient.invalidateQueries({ queryKey: ["github", "namespaces"] });
      }
    }
  });
};
