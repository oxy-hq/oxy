import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { type AuditEventRow, type AuditFilter, CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Operator-action audit feed for the workspace. Drives the Audit
 * tab in Cameras settings.
 *
 * The list is bounded server-side (default 50, max 500), so
 * polling is cheap. We keep `refetchInterval` off by default —
 * audit is a "what just happened" surface; operators reload by
 * clicking refresh, not by tab cadence. They get fresher data
 * via React Query invalidation when their own write triggers
 * the cache to bust.
 */
const useAuditEvents = (
  workspaceId: string | undefined,
  filter: AuditFilter = {},
  enabled = true
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<AuditEventRow[]>({
    queryKey: queryKeys.camera.auditEvents(
      effectiveWorkspaceId,
      filter.action_prefix,
      filter.target_id
    ),
    queryFn: () => CameraService.listAuditEvents(effectiveWorkspaceId, filter),
    enabled
  });
};

export default useAuditEvents;
