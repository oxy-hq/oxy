import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Fetch the workspace's active domain pack. The pack auto-seeds on
 * first read on the backend, so a fresh workspace returns the
 * default `restaurant` starter rather than 404. Callers use this
 * for two surfaces:
 *
 *   1. CameraRolePicker — reads `body.roles[]` to populate the
 *      Select. Avoids the hard-coded role list of pre-Phase-4 code.
 *   2. DomainPackCard / PackEditorDialog — renders the full body
 *      for editing.
 */
const useActivePack = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.activePack(effectiveWorkspaceId),
    queryFn: () => CameraService.getActivePack(effectiveWorkspaceId)
  });
};

export default useActivePack;
