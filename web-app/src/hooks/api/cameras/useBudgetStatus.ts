import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { type BudgetStatus, CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Workspace-level VLM budget snapshot — used to render the
 * Cameras dashboard's projected-spend banner.
 *
 * Refresh cadence: 5 minutes. The underlying numbers move slowly
 * (compliance reports trickle in on dwell triggers), so we keep
 * polling quiet. Reads are cheap server-side (one bounded
 * DuckLake SELECT), so the cadence is a UX choice, not a cost one.
 */
const useBudgetStatus = (workspaceId: string | undefined, enabled = true) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<BudgetStatus>({
    queryKey: queryKeys.camera.budgetStatus(effectiveWorkspaceId),
    queryFn: () => CameraService.getBudgetStatus(effectiveWorkspaceId),
    enabled,
    staleTime: 5 * 60 * 1000,
    refetchInterval: 5 * 60 * 1000
  });
};

export default useBudgetStatus;
