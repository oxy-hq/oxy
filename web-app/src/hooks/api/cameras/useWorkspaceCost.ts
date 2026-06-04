import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type CostRange } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Workspace-wide VLM cost rollup with per-box breakdown sorted
 * heaviest-first. Same 5min cadence + staleTime as
 * `useEdgeBoxCost` — operators don't watch this in real-time.
 *
 * The per-box breakdown lets the EdgeBoxesTable render its cost
 * column with one fetch for the whole table (rather than N
 * per-box fetches). Hook callers consuming a single box's cost
 * can use `useEdgeBoxCost` directly.
 */
const useWorkspaceCost = (workspaceId: string | undefined, range: CostRange = "24h") => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.workspaceCost(effectiveWorkspaceId, range),
    queryFn: () => CameraService.getWorkspaceCost(effectiveWorkspaceId, range),
    staleTime: 5 * 60 * 1000,
    refetchInterval: 5 * 60 * 1000
  });
};

export default useWorkspaceCost;
