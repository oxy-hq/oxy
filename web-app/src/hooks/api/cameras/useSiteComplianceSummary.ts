import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

type Args = {
  workspaceId: string | undefined;
  siteId: string | undefined;
  since?: string;
};

/**
 * Per-site rollup of compliance incidents, one row per camera.
 *
 * Returns cameras with zero reports too — the UI shows a clean
 * "0 incidents" state for cameras that are configured but haven't
 * produced violations, distinct from "no cameras here yet."
 *
 * Disabled when `siteId` is missing so the same hook can be used
 * while the page is still resolving its default site selection.
 */
const useSiteComplianceSummary = ({ workspaceId, siteId, since }: Args) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.complianceSummary(effectiveWorkspaceId, siteId ?? "", since),
    queryFn: () =>
      CameraService.listSiteComplianceSummary(effectiveWorkspaceId, siteId as string, {
        since
      }),
    enabled: !!siteId
  });
};

export default useSiteComplianceSummary;
