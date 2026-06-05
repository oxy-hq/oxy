import { useMutation } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";

const useUnifiPreview = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useMutation({
    // `apiKey` is optional — when omitted, the backend uses the workspace's
    // stored UniFi credential. The Scan-sites flow calls this with no key
    // to re-discover sites without forcing the operator to paste again.
    mutationFn: ({ apiKey }: { apiKey?: string } = {}) =>
      CameraService.unifiPreview(effectiveWorkspaceId, apiKey)
  });
};

export default useUnifiPreview;
