import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type PreparedDeviceResponse } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * "Add device" wizard backend caller. Mints a fresh device
 * identity + pre-creates the pending claim atomically; the
 * mutation result carries the install snippet the operator
 * pastes into the new box's SSH session.
 *
 * Invalidates the fleet devices list so the new pending row
 * appears immediately without a manual refresh.
 */
const usePrepareDevice = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<
    PreparedDeviceResponse,
    Error,
    { siteId: string; hardwareLabel?: string; tsAuthkey?: string }
  >({
    mutationFn: ({ siteId, hardwareLabel, tsAuthkey }) =>
      CameraService.prepareDevice(effectiveWorkspaceId, {
        site_id: siteId,
        hardware_label: hardwareLabel,
        // `null` instead of omitting so backend's `#[serde(default)]`
        // treats missing/empty identically. Trim happens on caller.
        ts_authkey: tsAuthkey && tsAuthkey.length > 0 ? tsAuthkey : undefined
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.fleetDevices(effectiveWorkspaceId)
      });
    }
  });
};

export default usePrepareDevice;
