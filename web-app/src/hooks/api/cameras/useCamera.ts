import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

const useCamera = (workspaceId: string | undefined, cameraId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.camera(effectiveWorkspaceId, cameraId ?? ""),
    queryFn: () => CameraService.getCamera(effectiveWorkspaceId, cameraId!),
    enabled: !!cameraId
  });
};

export default useCamera;
