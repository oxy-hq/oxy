import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

const useEdgeBox = (workspaceId: string | undefined, boxId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.edgeBox(effectiveWorkspaceId, boxId ?? ""),
    queryFn: () => CameraService.getEdgeBox(effectiveWorkspaceId, boxId!),
    enabled: !!boxId
  });
};

export default useEdgeBox;
