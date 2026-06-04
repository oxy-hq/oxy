import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type UpdateAction } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Commit the operator's diff-modal choices. Backend renders the
 * merged body against a dummy context before persisting; a bad
 * merge (e.g. operator took an upstream role whose template
 * references a variable they kept removed) surfaces as 400 via
 * the mutation's `error` field.
 *
 * On success: invalidate active-pack, packs list, and
 * starter-updates — all three change. The diff modal closes on
 * the caller side.
 */
const useApplyStarterUpdate = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      packKey,
      actions
    }: {
      packKey: string;
      actions: Record<string, UpdateAction>;
    }) => CameraService.applyStarterUpdate(effectiveWorkspaceId, packKey, actions),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.activePack(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.packs(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.starterUpdates(effectiveWorkspaceId)
      });
    }
  });
};

export default useApplyStarterUpdate;
