import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { apiClient } from "@/services/api/axios";
import queryKeys from "../queryKey";

/**
 * Toggle a camera's `analytics_consent`. The edge worker picks up
 * the change on its next config poll (every ~30s) — until then the
 * old value is still in effect, so the UI should treat this as
 * "eventually consistent" rather than instant.
 *
 * On success: invalidate the cameras list so the row's badge updates.
 */
const usePatchCameraConsent = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ cameraId, consent }: { cameraId: string; consent: boolean }) => {
      await apiClient.patch(`/${effectiveWorkspaceId}/cameras/${cameraId}/consent`, {
        analytics_consent: consent
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
      });
    }
  });
};

export default usePatchCameraConsent;
