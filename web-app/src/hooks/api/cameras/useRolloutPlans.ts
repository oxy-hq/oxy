import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type CreateRolloutBody } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Polling cadence on the rollout list. 30s is tight enough that the
 * supervisor's auto-promote / auto-abort transitions land in the
 * operator's view within one cycle without thrashing the backend.
 */
const ROLLOUT_REFETCH_MS = 30_000;

export const useRolloutConvergence = (
  workspaceId: string | undefined,
  planId: string | undefined
) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.rolloutConvergence(effectiveWorkspaceId, planId ?? ""),
    queryFn: () => CameraService.getRolloutConvergence(effectiveWorkspaceId, planId as string),
    enabled: !!planId,
    refetchInterval: ROLLOUT_REFETCH_MS
  });
};

export const useRolloutPlans = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.rolloutPlans(effectiveWorkspaceId),
    queryFn: () => CameraService.listRolloutPlans(effectiveWorkspaceId),
    refetchInterval: ROLLOUT_REFETCH_MS
  });
};

export const useCreateRolloutPlan = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateRolloutBody) =>
      CameraService.createRolloutPlan(effectiveWorkspaceId, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.camera.rolloutPlans(effectiveWorkspaceId) });
    }
  });
};

export const useAbortRolloutPlan = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ planId, reason }: { planId: string; reason?: string }) =>
      CameraService.abortRolloutPlan(effectiveWorkspaceId, planId, reason),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.camera.rolloutPlans(effectiveWorkspaceId) });
    }
  });
};
