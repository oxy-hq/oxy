import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Apply one target_image_tag (and optional held_until) to multiple
 * edge boxes atomically. The backend pre-validates workspace
 * ownership of every box_id before touching the DB — a single
 * cross-workspace id fails the whole call with 403.
 *
 * On success: invalidate the edge boxes list so each updated row
 * shows its new "pending update" state in one refetch.
 */
const useBulkPatchEdgeBoxTarget = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      boxIds,
      targetImageTag,
      heldUntil
    }: {
      boxIds: string[];
      targetImageTag: string | null;
      heldUntil?: string | null;
    }) =>
      CameraService.bulkPatchEdgeBoxTarget(effectiveWorkspaceId, {
        box_ids: boxIds,
        target_image_tag: targetImageTag,
        held_until: heldUntil ?? null
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.edgeBoxes(effectiveWorkspaceId)
      });
    }
  });
};

export default useBulkPatchEdgeBoxTarget;
