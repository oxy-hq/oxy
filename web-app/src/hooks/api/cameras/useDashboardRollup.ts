import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type DashboardRollup } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Single round-trip dashboard rollup — totals, hourly buckets, and
 * top-N cameras by detections/violations. Replaces the per-site
 * fan-out the Dashboard page used in v0 with one query whose cost
 * is independent of how many sites the workspace owns.
 *
 * Polls every 60s so KPIs refresh while the operator is on the page,
 * matching the cadence of the per-camera health summary so they
 * never visually disagree about the same camera by more than one
 * tick.
 */
const useDashboardRollup = (
  workspaceId: string | undefined,
  params: { siteId?: string | null; since?: string } = {},
  enabled = true
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<DashboardRollup>({
    queryKey: queryKeys.camera.dashboardRollup(
      effectiveWorkspaceId,
      params.siteId ?? null,
      params.since
    ),
    queryFn: () =>
      CameraService.getDashboardRollup(effectiveWorkspaceId, {
        since: params.since,
        site_id: params.siteId ?? null
      }),
    enabled,
    refetchInterval: 60_000
  });
};

export default useDashboardRollup;
