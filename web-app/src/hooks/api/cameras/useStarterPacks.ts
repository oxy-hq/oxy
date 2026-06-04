import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Fetch Oxy's curated starter library. The list is global to the
 * deployment but routed through the workspace path for consistency
 * with the rest of the camera-pack surface. The switch-pack dialog
 * uses this to render fork-able starters next to already-forked
 * packs.
 *
 * Cached liberally — the library only changes on backend deploys,
 * so the default 5min staleTime is appropriate here.
 */
const useStarterPacks = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.starterPacks(effectiveWorkspaceId),
    queryFn: () => CameraService.listStarters(effectiveWorkspaceId),
    staleTime: 5 * 60 * 1000
  });
};

export default useStarterPacks;
