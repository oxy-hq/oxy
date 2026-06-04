import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Set (or clear, by passing null) the monthly VLM budget for the
 * workspace. On success: invalidate the budget status so the
 * banner re-renders with the new ceiling and refreshed
 * percent-consumed/projected values.
 */
const useSetBudget = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (monthlyBudgetMicros: number | null) =>
      CameraService.setBudget(effectiveWorkspaceId, monthlyBudgetMicros),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.budgetStatus(effectiveWorkspaceId)
      });
    }
  });
};

export default useSetBudget;
