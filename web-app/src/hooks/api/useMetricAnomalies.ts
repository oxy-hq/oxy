import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { MetricAnomaliesService } from "@/services/api/metricAnomalies";
import type {
  AnomalyStatus,
  ListMonitorsResponse,
  MonitorCoverage,
  MonitorEntry
} from "@/types/metricAnomalies";
import type { ExplainResult } from "@/types/metricTree";
import queryKeys from "./queryKey";

/** Anomalies for the current workspace, optionally filtered by status. */
export function useMetricAnomalies(status?: AnomalyStatus) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery({
    queryKey: queryKeys.metricAnomalies.list(projectId, status),
    queryFn: () => MetricAnomaliesService.list(projectId, status),
    retry: false
  });
}

/** Trigger a scan; surfaces a success toast with monitor counts on completion.
 *  Pass `asOf` (YYYY-MM-DD) to scan against a past date — useful when the
 *  demo dataset doesn't reach today.
 *
 *  When `pending: true` is returned the scan is still running in the
 *  background. We poll the anomalies list at increasing intervals so the
 *  inbox updates automatically once results land. */
export function useScanMetricAnomalies() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (asOf?: string) => MetricAnomaliesService.scan(projectId, asOf),
    onSuccess: (data) => {
      if (data.pending) {
        toast.info("Scan started — anomalies will appear shortly.");
        // Poll at 5 s, 15 s, 35 s, and 75 s to pick up background results.
        for (const delayMs of [5_000, 15_000, 35_000, 75_000]) {
          setTimeout(
            () => qc.invalidateQueries({ queryKey: queryKeys.metricAnomalies.all }),
            delayMs
          );
        }
        return;
      }
      const monitors = data.monitors_scanned;
      const failed = data.monitors_failed;
      const persisted = data.anomalies_persisted;
      if (failed > 0) {
        const failures = data.failures ?? [];
        // Name the first few failed monitors + their error in the toast; the
        // inbox banner carries the full list. Sonner collapses newlines, so
        // join with a middot rather than "\n".
        const preview = failures
          .slice(0, 3)
          .map((f) => {
            const name = f.label || f.measure;
            const seg = f.dimension_key ? ` [${f.dimension_key}]` : "";
            return `${name}${seg}: ${clampError(f.error)}`;
          })
          .join(" · ");
        const more = failures.length > 3 ? ` · +${failures.length - 3} more` : "";
        toast.warning(
          `Scanned ${monitors} monitor${monitors === 1 ? "" : "s"} (${failed} failed). ${persisted} anomalies persisted.`,
          { description: preview ? `${preview}${more}` : undefined, duration: 10_000 }
        );
      } else {
        toast.success(
          `Scanned ${monitors} monitor${monitors === 1 ? "" : "s"}. ${persisted} anomalies persisted.`
        );
      }
      qc.invalidateQueries({ queryKey: queryKeys.metricAnomalies.all });
    },
    onError: (e) => {
      toast.error(`Scan failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  });
}

/** Clamp a single error message for the toast preview so one verbose
 *  `ScanError` chain doesn't swamp the middot-joined list. The inbox banner
 *  carries the full, untruncated error. */
function clampError(error: string, max = 120): string {
  return error.length > max ? `${error.slice(0, max - 1)}…` : error;
}

/** Mark an anomaly acknowledged / dismissed / new. */
export function useUpdateAnomalyStatus() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, status }: { id: string; status: AnomalyStatus }) =>
      MetricAnomaliesService.updateStatus(projectId, id, status),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.metricAnomalies.all });
    },
    onError: (e) => {
      toast.error(`Failed to update status: ${e instanceof Error ? e.message : String(e)}`);
    }
  });
}

/** Cached server-side explain for a single anomaly. The first call runs
 *  airlayer's recursive decomposition and writes the result onto the
 *  row; every later call (including after a page refresh) returns the
 *  cached payload instantly. Pair with `useRefreshAnomalyExplain` for
 *  the bust-cache button. */
export function useAnomalyExplain(anomalyId: string | null) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<ExplainResult>({
    queryKey: anomalyId
      ? queryKeys.metricAnomalies.explain(projectId, anomalyId)
      : ([...queryKeys.metricAnomalies.all, "explain", "idle"] as const),
    queryFn: () => {
      if (!anomalyId) throw new Error("anomalyId required");
      return MetricAnomaliesService.explain(projectId, anomalyId);
    },
    enabled: !!anomalyId,
    // No staleTime — the server is the source of truth. React Query keeps
    // the cached payload in memory for the session, then re-fetches on
    // mount (server returns cached row, so still effectively instant).
    retry: false
  });
}

/** Shared fetch behind {@link useMonitors} and {@link useMonitorCoverage}.
 *
 *  One endpoint, one cache entry, two selectors — React Query dedupes on the
 *  shared key, so the tab does not fetch this twice.
 *
 *  The config half only changes on deploy/edit, but the coverage half moves
 *  with every scan, so this is no longer `staleTime: Infinity`. */
function useMonitorsQuery<T>(select: (data: ListMonitorsResponse) => T) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery({
    queryKey: queryKeys.metricAnomalies.monitors(projectId),
    queryFn: () => MetricAnomaliesService.listMonitors(projectId),
    staleTime: 60_000,
    select
  });
}

/** Monitor configurations from `.monitor.yml`. Returns an empty array when no
 *  file is configured. */
export function useMonitors() {
  return useMonitorsQuery((data) => data.monitors ?? ([] as MonitorEntry[]));
}

/** Per-segment scan coverage — which monitors are being scored and which are
 *  still accumulating history. Empty until the workspace has been scanned. */
export function useMonitorCoverage() {
  return useMonitorsQuery((data) => data.coverage ?? ([] as MonitorCoverage[]));
}

/** Force-refresh the cached explain for an anomaly. Calls the same
 *  endpoint with `?refresh=true`, then writes the fresh payload into
 *  the React Query cache so subscribers re-render. */
export function useRefreshAnomalyExplain() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (anomalyId: string) => MetricAnomaliesService.explain(projectId, anomalyId, true),
    onSuccess: (data, anomalyId) => {
      qc.setQueryData(queryKeys.metricAnomalies.explain(projectId, anomalyId), data);
    },
    onError: (e) => {
      toast.error(`Refresh failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  });
}
