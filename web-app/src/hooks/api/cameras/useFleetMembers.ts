import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type FleetMemberRow } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Merged "physical boxes" feed — every edge_box the operator
 * owns plus any pending device claims that haven't materialized
 * an edge_box yet. Replaces the previously-split useEdgeBoxes +
 * useFleetDevices pair.
 *
 * 30s poll matches the fleet/devices cadence: an operator who
 * just clicked Add device wants to see the pending row appear
 * without a manual refresh, but the data doesn't change often
 * enough to warrant tighter polling.
 */
const useFleetMembers = (workspaceId: string | undefined, enabled = true) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<FleetMemberRow[]>({
    queryKey: queryKeys.camera.fleetMembers(effectiveWorkspaceId),
    queryFn: () => CameraService.listFleetMembers(effectiveWorkspaceId),
    enabled,
    refetchInterval: 30_000
  });
};

export default useFleetMembers;
