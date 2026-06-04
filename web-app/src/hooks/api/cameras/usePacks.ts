import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Fetch every pack in this workspace (active + inactive). The
 * switch-pack dialog reads this to show "already forked" packs
 * with an Activate button, distinct from starters the operator
 * could still fork. Body is omitted from the response — open the
 * editor for the active pack to fetch it.
 */
const usePacks = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.packs(effectiveWorkspaceId),
    queryFn: () => CameraService.listPacks(effectiveWorkspaceId)
  });
};

export default usePacks;
