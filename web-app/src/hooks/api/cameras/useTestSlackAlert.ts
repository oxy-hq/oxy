import { useMutation } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { apiClient } from "@/services/api/axios";

/**
 * Fires a test Slack alert against the workspace's configured
 * channel. The backend returns `{delivered: false}` (not an error)
 * when there's no Slack installation — the UI surfaces a "configure
 * Slack" hint in that case rather than a red toast.
 */
const useTestSlackAlert = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useMutation<{ delivered: boolean }, Error, void>({
    mutationFn: async () => {
      const response = await apiClient.post<{ delivered: boolean }>(
        `/${effectiveWorkspaceId}/fleet/test-alert`
      );
      return response.data;
    }
  });
};

export default useTestSlackAlert;
