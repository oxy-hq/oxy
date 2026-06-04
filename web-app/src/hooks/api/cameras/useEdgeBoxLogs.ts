import { useQuery } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type LogFilter, type LogRow } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Pull recent log rows for one edge box.
 *
 * The reader is filtered server-side by severity and event-name
 * substring; tail-mode polling lives on the call site via the
 * `refetchInterval` arg (operators flip this on/off from the
 * viewer's "auto-refresh" toggle).
 *
 * **Why not an SSE stream**: V1 ships request/response polling
 * because the read endpoint is cheap (one bounded SELECT against
 * a single DuckLake table) and the operator viewer hits it at
 * 3-5s cadence at most. SSE would add a long-lived
 * `connect_for_reads` mint per browser tab, which is more cost
 * than warranted until we have a real "tail multiple boxes at
 * once" use case.
 */
const useEdgeBoxLogs = (
  workspaceId: string | undefined,
  boxId: string | undefined,
  filter: LogFilter = {},
  options: { enabled?: boolean; refetchIntervalMs?: number | false } = {}
) => {
  const { enabled = true, refetchIntervalMs = false } = options;
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<LogRow[]>({
    queryKey: queryKeys.camera.edgeBoxLogs(
      effectiveWorkspaceId,
      boxId ?? "",
      filter.min_severity,
      filter.event_contains
    ),
    queryFn: () =>
      // biome-ignore lint/style/noNonNullAssertion: `enabled` gates the call until boxId is defined
      CameraService.getEdgeBoxLogs(effectiveWorkspaceId, boxId!, filter),
    enabled: enabled && !!boxId,
    refetchInterval: refetchIntervalMs
  });
};

export default useEdgeBoxLogs;
