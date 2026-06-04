import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Approve the pending zone/line proposal — copies proposed_*_json
 * into the live columns and clears the proposal. Invalidates the
 * camera list so the row's "proposal pending" badge disappears
 * without a manual refresh.
 */
const useApproveCameraProposal = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (cameraId: string) =>
      CameraService.approveCameraProposal(effectiveWorkspaceId, cameraId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
      });
    }
  });
};

export default useApproveCameraProposal;
