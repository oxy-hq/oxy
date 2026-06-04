import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { apiClient } from "@/services/api/axios";
import queryKeys from "../queryKey";

/**
 * Set a camera's role tag. The worker uses this to pick a VLM
 * prompt; `null` clears and falls back to the pack's "other" role
 * (or the first role if no `other` is defined).
 *
 * Phase 4 (this revision): `role` is `string | null` rather than a
 * literal union — the active domain pack defines the valid set,
 * and `KNOWN_ROLES` on the backend has been replaced with delegation
 * to the active pack. Callers usually source `role` from
 * `useActivePack().body.roles`, so the right shape just falls out.
 * The backend still rejects unknown values with 400.
 */
const usePatchCameraRole = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ cameraId, role }: { cameraId: string; role: string | null }) => {
      await apiClient.patch(`/${effectiveWorkspaceId}/cameras/${cameraId}/role`, {
        role
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
      });
    }
  });
};

export default usePatchCameraRole;
