import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

/**
 * UniFi import mutation. On success, invalidates all three list query
 * keys for this workspace so the Cameras dashboard tabs refresh with
 * the freshly imported sites / edge boxes / cameras.
 */
const useUnifiImport = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ apiKey, siteFilter }: { apiKey: string; siteFilter?: string }) =>
      CameraService.unifiImport(effectiveWorkspaceId, apiKey, siteFilter),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.sites(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.edgeBoxes(effectiveWorkspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.cameras(effectiveWorkspaceId)
      });
    }
  });
};

export default useUnifiImport;
