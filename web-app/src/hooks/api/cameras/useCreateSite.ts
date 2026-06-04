import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Create a manually-provisioned site. Companion to the UniFi
 * import flow — the resulting row has `source: "manual"` and
 * won't be touched by `oxy build` re-syncs.
 *
 * On success: invalidate the sites list so the new row appears
 * immediately, and the edge-boxes table picks up the new
 * site-name option in its filter.
 */
const useCreateSite = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: { name: string; timezone?: string; region?: string | null }) =>
      CameraService.createSite(effectiveWorkspaceId, body),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.sites(effectiveWorkspaceId)
      });
    }
  });
};

export default useCreateSite;
