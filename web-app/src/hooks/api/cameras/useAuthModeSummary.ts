import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

/**
 * Fleet auth-mode rollup used by the Edge JWT-cutover banner.
 *
 * Polls slowly (60s) — operators don't need real-time on this,
 * and the underlying counts only change when an edge box first
 * authenticates with JWT. Refetching on window focus catches
 * "I just rebooted my last bearer box."
 */
const useAuthModeSummary = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.authModeSummary(effectiveWorkspaceId),
    queryFn: () => CameraService.getAuthModeSummary(effectiveWorkspaceId),
    refetchInterval: 60_000,
    refetchOnWindowFocus: true
  });
};

export default useAuthModeSummary;
