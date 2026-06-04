import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Detect packs in this workspace that have an upstream update
 * available. Empty array = nothing to do. The card on
 * `Settings → Cameras → Domain pack` reads the length to show
 * the update badge.
 *
 * Refetches on a 10-minute interval — fresh enough that operators
 * notice updates within a coffee break, but not so aggressive that
 * we hammer the backend's pack-diff path for inactive workspaces.
 */
const useStarterUpdates = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.starterUpdates(effectiveWorkspaceId),
    queryFn: () => CameraService.listStarterUpdates(effectiveWorkspaceId),
    refetchInterval: 10 * 60 * 1000
  });
};

export default useStarterUpdates;
