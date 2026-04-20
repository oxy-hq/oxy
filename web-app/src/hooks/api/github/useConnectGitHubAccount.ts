import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { GitHubApiService } from "@/services/api";
import { openSecureWindow } from "@/utils/githubAppInstall";
import { waitForGitHubCallback } from "@/utils/githubCallbackMessage";

/**
 * Opens a popup to GitHub OAuth, waits for the popup's /github/oauth-callback
 * page to postMessage success, then refetches the account.
 */
export const useConnectGitHubAccount = () => {
  const queryClient = useQueryClient();
  return useMutation<void, Error, { orgId: string }>({
    mutationFn: async ({ orgId }) => {
      const url = await GitHubApiService.getOauthUrl(orgId, window.location.origin);
      const popup = openSecureWindow(url);
      await waitForGitHubCallback(popup, "oauth");
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.github.account });
    }
  });
};
