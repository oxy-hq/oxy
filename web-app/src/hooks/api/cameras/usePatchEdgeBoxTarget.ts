import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Set a per-edge-box target image tag (IoT Phase 2 software-update
 * channel). The device's update agent polls
 * `/control/boxes/{id}/target` every ~5 minutes and pulls + restarts
 * the worker container when this changes. Passing
 * `target_image_tag: null` clears the target and disables automated
 * updates for the box.
 *
 * On success: invalidate the edge boxes list so the new
 * `target_image_tag` + `held_until` show up in the table without
 * a manual refetch.
 */
const usePatchEdgeBoxTarget = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      boxId,
      targetImageTag,
      heldUntil
    }: {
      boxId: string;
      targetImageTag: string | null;
      heldUntil?: string | null;
    }) =>
      CameraService.patchEdgeBoxTarget(effectiveWorkspaceId, boxId, {
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

export default usePatchEdgeBoxTarget;
