import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { MetricTreeService } from "@/services/api/metricTree";
import type { DistributionRequest, ExplainRequest } from "@/types/metricTree";
import queryKeys from "./queryKey";

/** The metric tree for the current project, optionally rooted at `root`. */
export function useMetricTree(root?: string) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.metricTree.tree(projectId, root),
    queryFn: () => MetricTreeService.getTree(projectId, root),
    retry: false
  });
}

/** Ranked drivers of `measureId`. Disabled until a measure is selected. */
export function useSensitivity(measureId: string | undefined) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.metricTree.sensitivity(projectId, measureId),
    queryFn: () => {
      if (!measureId) throw new Error("A measure is required");
      return MetricTreeService.getSensitivity(projectId, measureId);
    },
    enabled: !!measureId,
    retry: false
  });
}

/** Cached period-over-period explain. Used by the Insights-inbox drawer
 *  so reopening the same anomaly reuses the result instead of re-running
 *  the recursive search (which can take 20-30s on warehouse-scale data).
 *
 *  `enabled` lets the caller hold the query off until they have a request
 *  to run (e.g. the drawer passes the anomaly's derived request once it
 *  has loaded). */
export function useExplainQuery(request: ExplainRequest | null, enabled = true) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: request
      ? queryKeys.metricTree.explain(
          projectId,
          request.target,
          request.time_dimension,
          request.current_period,
          request.previous_period,
          request.config?.deep ?? false
        )
      : ([...queryKeys.metricTree.all, "explain", "idle"] as const),
    queryFn: () => {
      if (!request) throw new Error("explain request is required");
      return MetricTreeService.explain(projectId, request);
    },
    enabled: enabled && !!request,
    // 5 min — long enough for repeated drawer open/close on the same
    // anomaly to hit cache; short enough that a manual rescan picks up
    // fresh warehouse data quickly.
    staleTime: 5 * 60 * 1000,
    retry: false
  });
}

/** Per-view time dimensions discovered from the semantic layer. Cached for
 *  the session — schema rarely changes within a single visit. */
export function useTimeDimensions() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.metricTree.timeDimensions(projectId),
    queryFn: () => MetricTreeService.timeDimensions(projectId),
    staleTime: 5 * 60 * 1000,
    retry: false
  });
}

/** Single-period distribution for a measure. The baseline window is
 *  auto-derived server-side. Pass `null` to disable. */
export function useDistributionQuery(request: DistributionRequest | null, enabled = true) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: request
      ? queryKeys.metricTree.distribution(
          projectId,
          request.target,
          request.time_dimension,
          request.period
        )
      : ([...queryKeys.metricTree.all, "distribution", "idle"] as const),
    queryFn: () => {
      if (!request) throw new Error("distribution request is required");
      return MetricTreeService.distribution(projectId, request);
    },
    enabled: enabled && !!request,
    staleTime: 5 * 60 * 1000,
    retry: false
  });
}
