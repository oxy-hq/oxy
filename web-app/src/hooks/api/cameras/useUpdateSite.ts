import { useMutation, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type Site } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Patch a site's editable fields (today: public_ip, region, timezone).
 * Each field is `undefined`-means-no-change / `null`-means-clear — see
 * the backend's serde double-option pattern.
 *
 * Invalidates the sites list so the dropdown + table reflect the new
 * value immediately.
 */
const useUpdateSite = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<
    Site,
    Error,
    {
      siteId: string;
      body: {
        public_ip?: string | null;
        region?: string | null;
        timezone?: string;
      };
    }
  >({
    mutationFn: ({ siteId, body }) => CameraService.updateSite(effectiveWorkspaceId, siteId, body),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.sites(effectiveWorkspaceId)
      });
    }
  });
};

export default useUpdateSite;
