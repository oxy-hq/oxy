import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type RecentAlertRow } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Most-recent camera-health transitions, enriched with camera + site
 * names server-side. Drives the Dashboard's "Recent alerts" feed.
 * Independent of whether the workspace has a Slack install — the
 * alerter writes audit rows for every transition regardless of
 * delivery, so the feed always reflects reality.
 */
const useRecentAlerts = (
  workspaceId: string | undefined,
  params: { siteId?: string | null; limit?: number } = {},
  enabled = true
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const limit = params.limit ?? 20;
  return useQuery<RecentAlertRow[]>({
    queryKey: queryKeys.camera.recentAlerts(effectiveWorkspaceId, params.siteId ?? null, limit),
    queryFn: () =>
      CameraService.listRecentAlerts(effectiveWorkspaceId, {
        site_id: params.siteId ?? null,
        limit
      }),
    enabled,
    refetchInterval: 60_000
  });
};

export default useRecentAlerts;
