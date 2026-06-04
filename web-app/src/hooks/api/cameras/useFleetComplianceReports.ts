import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type ComplianceReport } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Workspace-wide compliance reports — newest first, one round-trip
 * regardless of camera count. Drives the Detections page (replaces
 * per-camera fan-out via `useQueries`).
 *
 * `keepPreviousData` keeps the table visible while the user adjusts
 * filters (range, site picker, etc.) — avoids the table blanking on
 * every keystroke / dropdown change.
 */
const useFleetComplianceReports = (
  workspaceId: string | undefined,
  params: { siteId?: string | null; since?: string; limit?: number } = {},
  enabled = true
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const limit = params.limit ?? 200;
  return useQuery<ComplianceReport[]>({
    queryKey: queryKeys.camera.fleetComplianceReports(
      effectiveWorkspaceId,
      params.siteId ?? null,
      params.since,
      limit
    ),
    queryFn: () =>
      CameraService.listFleetComplianceReports(effectiveWorkspaceId, {
        site_id: params.siteId ?? null,
        since: params.since,
        limit
      }),
    enabled,
    placeholderData: keepPreviousData,
    refetchInterval: 60_000
  });
};

export default useFleetComplianceReports;
