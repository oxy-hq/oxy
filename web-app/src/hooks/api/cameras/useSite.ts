import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

const useSite = (workspaceId: string | undefined, siteId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.site(effectiveWorkspaceId, siteId ?? ""),
    queryFn: () => CameraService.getSite(effectiveWorkspaceId, siteId!),
    enabled: !!siteId
  });
};

export default useSite;
