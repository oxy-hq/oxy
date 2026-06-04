import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { type CameraHealthRow, CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Per-camera health rollup — feeds the status badge in
 * CamerasTable. Polls every 60s so a degraded camera surfaces
 * before the next operator visit, but rare enough that the
 * Airhouse SELECT isn't on the hot path. The query is cheap
 * (one bounded SELECT against the most-recent row per camera)
 * so we could shorten the interval if customers ask.
 */
const useCameraHealthSummary = (workspaceId: string | undefined, enabled = true) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<CameraHealthRow[]>({
    queryKey: queryKeys.camera.cameraHealthSummary(effectiveWorkspaceId),
    queryFn: () => CameraService.getCameraHealthSummary(effectiveWorkspaceId),
    enabled,
    refetchInterval: 60_000
  });
};

export default useCameraHealthSummary;
