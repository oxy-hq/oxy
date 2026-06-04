import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

const useEdgeBoxes = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.edgeBoxes(effectiveWorkspaceId),
    queryFn: () => CameraService.listEdgeBoxes(effectiveWorkspaceId)
  });
};

export default useEdgeBoxes;
