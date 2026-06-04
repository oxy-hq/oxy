import { useMutation } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";

const useUnifiPreview = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useMutation({
    mutationFn: ({ apiKey }: { apiKey: string }) =>
      CameraService.unifiPreview(effectiveWorkspaceId, apiKey)
  });
};

export default useUnifiPreview;
