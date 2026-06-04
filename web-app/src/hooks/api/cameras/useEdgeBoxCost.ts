import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type CostRange } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Per-box VLM cost rollup over the requested window. Reads from
 * `compliance_reports.tokens_used` in Airhouse, applies the
 * versioned price table on the backend, returns USD micros plus a
 * per-model breakdown.
 *
 * Refresh cadence: 5 minutes. Cost is a slow-moving signal —
 * the underlying VLM calls fire only on dwell triggers — so we
 * don't need real-time. Keeps the API quiet while still surfacing
 * a recent number when the operator opens the page.
 */
const useEdgeBoxCost = (
  workspaceId: string | undefined,
  boxId: string | undefined,
  range: CostRange = "24h",
  enabled = true
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.edgeBoxCost(effectiveWorkspaceId, boxId ?? "", range),
    queryFn: () =>
      // biome-ignore lint/style/noNonNullAssertion: `enabled` gates the call until boxId is defined
      CameraService.getEdgeBoxCost(effectiveWorkspaceId, boxId!, range),
    enabled: enabled && !!boxId,
    staleTime: 5 * 60 * 1000,
    refetchInterval: 5 * 60 * 1000
  });
};

export default useEdgeBoxCost;
