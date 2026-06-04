import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Activate one of the workspace's existing packs. The previously-
 * active pack is deactivated transactionally; the partial unique
 * index on `is_active` guarantees there's never more than one
 * active row at rest.
 *
 * On success: invalidate active-pack + packs list. Also worth
 * invalidating the cameras query — a switched pack may change
 * the valid role set, and stale `cameras.role` values silently
 * fall through to the pack's `other` role on the worker. We
 * leave that as a UI follow-up.
 */
const useActivatePack = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ packKey }: { packKey: string }) =>
      CameraService.activatePack(effectiveWorkspaceId, packKey),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.activePack(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.packs(effectiveWorkspaceId)
      });
    }
  });
};

export default useActivatePack;
