import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { type CameraComplianceSummary, CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Per-camera compliance totals across the workspace (or one site)
 * in a single Airhouse round-trip. Drives the Playback Grid view —
 * replaces a per-site `useQueries` fan-out that minted one
 * Airhouse credential per site on every refetch.
 */
const useFleetComplianceSummary = (
  workspaceId: string | undefined,
  params: { siteId?: string | null; since?: string } = {},
  enabled = true
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<CameraComplianceSummary[]>({
    queryKey: queryKeys.camera.fleetComplianceSummary(
      effectiveWorkspaceId,
      params.siteId ?? null,
      params.since
    ),
    queryFn: () =>
      CameraService.listFleetComplianceSummary(effectiveWorkspaceId, {
        site_id: params.siteId ?? null,
        since: params.since
      }),
    enabled,
    placeholderData: keepPreviousData,
    refetchInterval: 60_000
  });
};

export default useFleetComplianceSummary;
