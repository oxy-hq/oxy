import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Retroactive claim: attach a known `device_id` to an existing
 * `edge_box_id` without running the announce → bootstrap dance.
 * Used for legacy bearer-auth boxes that pre-date the IoT
 * identity layer; the operator reads the device's UUID off the
 * worker startup log and pastes it in.
 *
 * On success: refresh the fleet list (the box now shows up under
 * Fleet) and the edge boxes list (no fields change, but it keeps
 * the cached row consistent for future operations).
 */
const useBindExistingDevice = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ deviceId, edgeBoxId }: { deviceId: string; edgeBoxId: string }) =>
      CameraService.bindExistingDevice(effectiveWorkspaceId, {
        device_id: deviceId,
        edge_box_id: edgeBoxId
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.fleetDevices(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.edgeBoxes(effectiveWorkspaceId)
      });
    }
  });
};

export default useBindExistingDevice;
