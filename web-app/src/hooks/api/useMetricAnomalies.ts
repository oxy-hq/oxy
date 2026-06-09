import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { MetricAnomaliesService } from "@/services/api/metricAnomalies";
import type { AnomalyStatus, MonitorEntry } from "@/types/metricAnomalies";
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
        toast.warning(
          `Scanned ${monitors} monitor${monitors === 1 ? "" : "s"} (${failed} failed). ${persisted} anomalies persisted.`
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

/** Monitor configurations from `.monitor.yml`. Fetched once per session
 *  (staleTime: Infinity — the file only changes on deploy/edit).
 *  Returns an empty array when no file is configured. */
export function useMonitors() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery({
    queryKey: queryKeys.metricAnomalies.monitors(projectId),
    queryFn: () => MetricAnomaliesService.listMonitors(projectId),
    staleTime: Infinity,
    placeholderData: [] as MonitorEntry[]
  });
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
