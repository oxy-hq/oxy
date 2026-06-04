import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { type BulkRewriteResult, CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Bulk-rewrite each camera's RTSP URL onto the site's public_ip + UniFi
 * WAN port 7447. Returns a per-line outcome so the UI can render a
 * mixed-success result table.
 *
 * Invalidates the cameras list on success because rewritten rows now
 * have a different `rtsp_url`.
 */
const useBulkRewriteSiteCameraRtsp = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<BulkRewriteResult, Error, { siteId: string; lines: string[] }>({
    mutationFn: ({ siteId, lines }) =>
      CameraService.bulkRewriteSiteCameraRtsp(effectiveWorkspaceId, siteId, lines),
    onSuccess: (data) => {
      if (data.summary.rewritten > 0) {
        queryClient.invalidateQueries({
          queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
        });
      }
    }
  });
};

export default useBulkRewriteSiteCameraRtsp;
