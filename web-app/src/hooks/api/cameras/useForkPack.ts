import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Fork a starter into the workspace as a new pack. The new pack
 * becomes active — the previous active pack is deactivated
 * transactionally on the backend. On success we invalidate both
 * the active-pack query and the workspace-packs list so the UI
 * reflects the new state without a manual refetch.
 *
 * Conflict (`409` / 502 today) means the operator already forked
 * this starter — the caller should switch to `useActivatePack`
 * against the existing pack instead. v1 ships without that
 * pre-flight; the UI's "Activate" button under already-forked
 * packs covers the same intent.
 */
const useForkPack = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ starterKey }: { starterKey: string }) =>
      CameraService.forkPack(effectiveWorkspaceId, starterKey),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.activePack(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.packs(effectiveWorkspaceId)
      });
    }
  });
};

export default useForkPack;
