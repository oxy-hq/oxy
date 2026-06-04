import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import {
  type ArbitrationRow,
  type ArbitrationVerdict,
  CameraService
} from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Look up the operator arbitration for a compliance report, if any.
 * Returns null when no verdict has been recorded.
 */
export const useArbitration = (workspaceId: string | undefined, reportId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<ArbitrationRow | null>({
    queryKey: queryKeys.camera.arbitration(effectiveWorkspaceId, reportId ?? ""),
    queryFn: () => CameraService.getArbitration(effectiveWorkspaceId, reportId ?? ""),
    enabled: !!reportId
  });
};

/**
 * Upsert an arbitration. Invalidates the per-report lookup so the
 * verdict pills + buttons reflect the new state immediately.
 */
export const usePutArbitration = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<
    ArbitrationRow,
    Error,
    { reportId: string; verdict: ArbitrationVerdict; notes?: string | null }
  >({
    mutationFn: ({ reportId, verdict, notes }) =>
      CameraService.putArbitration(effectiveWorkspaceId, {
        report_id: reportId,
        verdict,
        notes: notes ?? null
      }),
    onSuccess: (data) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.arbitration(effectiveWorkspaceId, data.report_id)
      });
    }
  });
};

/**
 * Clear an existing arbitration. Idempotent — the operator changed
 * their mind or wants to re-investigate. Same invalidation shape as
 * the upsert hook.
 */
export const useDeleteArbitration = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<void, Error, { reportId: string }>({
    mutationFn: ({ reportId }) => CameraService.deleteArbitration(effectiveWorkspaceId, reportId),
    onSuccess: (_, { reportId }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.arbitration(effectiveWorkspaceId, reportId)
      });
    }
  });
};
