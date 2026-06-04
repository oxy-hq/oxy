import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type RemoveFleetMemberResult } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Operator-driven removal: retire an edge box + revoke its
 * claim/bearer, or revoke a pending claim. Invalidates the
 * merged fleet list and the legacy edge_boxes list so the row
 * disappears immediately.
 */
const useRemoveFleetMember = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<
    RemoveFleetMemberResult,
    Error,
    { edgeBoxId?: string | null; deviceId?: string | null }
  >({
    mutationFn: ({ edgeBoxId, deviceId }) =>
      CameraService.removeFleetMember(effectiveWorkspaceId, {
        edge_box_id: edgeBoxId,
        device_id: deviceId
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.fleetMembers(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.edgeBoxes(effectiveWorkspaceId)
      });
    }
  });
};

export default useRemoveFleetMember;
