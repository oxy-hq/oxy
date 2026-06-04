import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type ResyncPublicIpResult } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Targeted UniFi resync for one site's public_ip. Cheap alternative
 * to running the full UniFi import when an ISP rotation broke the
 * pre-edge-box RTSP rewrites — just refreshes `sites.public_ip` and
 * the controller's mirror.
 *
 * Invalidates the sites list on success so the table reflects the
 * new value (and the rewrite UI re-enables itself if it had been
 * blocked on a missing IP).
 */
const useResyncSitePublicIp = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<
    ResyncPublicIpResult,
    Error,
    // `apiKey` is optional now — when omitted, the backend's
    // `resolve_api_key` falls back to the workspace's stored UniFi
    // credential. Inline key still works for first-time setup.
    { siteId: string; apiKey?: string | null }
  >({
    mutationFn: ({ siteId, apiKey }) =>
      CameraService.resyncSitePublicIp(effectiveWorkspaceId, siteId, apiKey),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.sites(effectiveWorkspaceId)
      });
    }
  });
};

export default useResyncSitePublicIp;
