import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService } from "@/services/api";
import queryKeys from "../queryKey";

type Args = {
  workspaceId: string | undefined;
  cameraId: string | undefined;
  since?: string;
  limit?: number;
};

/**
 * Fetch compliance reports for a single camera, newest first.
 *
 * The backend goes through the Airhouse broker (read-side credential
 * minted with `SystemPurpose::ComplianceReportsRead`), so a tenant
 * that's never had an edge box write a report returns an empty
 * array, NOT an error — the schema may simply not exist yet.
 *
 * Disabled when either id is missing, so the same hook can be used
 * from a page that mounts before a camera has been selected.
 */
const useComplianceReports = ({ workspaceId, cameraId, since, limit }: Args) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery({
    queryKey: queryKeys.camera.complianceReports(
      effectiveWorkspaceId,
      cameraId ?? "",
      since,
      limit
    ),
    queryFn: () =>
      CameraService.listComplianceReports(effectiveWorkspaceId, cameraId as string, {
        since,
        limit
      }),
    enabled: !!cameraId
  });
};

export default useComplianceReports;
