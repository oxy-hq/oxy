import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Manually add a camera by RTSP URL. Companion to the UniFi-imported
 * cameras — operator types `rtsp://...` instead of having it
 * harvested from the UniFi controller. The auto-claim path binds
 * the camera to the first edge box at the same site on its next
 * config poll.
 *
 * Edge boxes already at the site will see the camera within
 * ~30s (their config poll cadence).
 */
const useCreateCamera = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: {
      site_id: string;
      name: string;
      rtsp_url: string;
      substream_url?: string | null;
      credentials_ref?: string | null;
    }) => CameraService.createCamera(effectiveWorkspaceId, body),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
      });
    }
  });
};

export default useCreateCamera;
