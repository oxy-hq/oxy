import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Set an edge box's cohort label. Used by the inline-edit cell in
 * the EdgeBoxes table; the rollout wizard reads distinct cohort
 * values to populate its canary picker.
 *
 * Invalidates both `edgeBoxes` and `fleetMembers` since both query
 * shapes carry the cohort field — without the second invalidation
 * the inline edit looks reverted on the next list refresh.
 */
const usePatchEdgeBoxCohort = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ boxId, cohort }: { boxId: string; cohort: string }) =>
      CameraService.patchEdgeBoxCohort(effectiveWorkspaceId, boxId, cohort),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.camera.edgeBoxes(effectiveWorkspaceId) });
      qc.invalidateQueries({ queryKey: queryKeys.camera.fleetMembers(effectiveWorkspaceId) });
    }
  });
};

export default usePatchEdgeBoxCohort;
