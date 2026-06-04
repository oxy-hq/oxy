import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { apiClient } from "@/services/api/axios";
import queryKeys from "../queryKey";

type PatchPayload = {
  cameraId: string;
  zones_json?: unknown;
  lines_json?: unknown;
};

/**
 * Push a new zones / lines geometry to a camera. The edge worker
 * picks up the change on its next config poll (~30s); the reader
 * for that camera will be stopped and restarted to apply the new
 * triggers via `_cam_signature`.
 *
 * Either field can be omitted to leave it unchanged — the backend
 * `PatchZonesBody` treats both as `Option<JsonValue>` so a missing
 * field is "don't touch."
 */
const usePatchCameraZones = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ cameraId, zones_json, lines_json }: PatchPayload) => {
      await apiClient.patch(`/${effectiveWorkspaceId}/cameras/${cameraId}/zones`, {
        zones_json,
        lines_json
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
      });
    }
  });
};

export default usePatchCameraZones;
