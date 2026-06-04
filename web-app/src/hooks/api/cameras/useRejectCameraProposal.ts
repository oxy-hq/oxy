import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Drop the pending zone/line proposal without touching live
 * geometry. Same cache invalidation as approve.
 */
const useRejectCameraProposal = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (cameraId: string) =>
      CameraService.rejectCameraProposal(effectiveWorkspaceId, cameraId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
      });
    }
  });
};

export default useRejectCameraProposal;
