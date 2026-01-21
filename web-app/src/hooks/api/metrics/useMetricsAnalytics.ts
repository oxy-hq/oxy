import { useQuery } from "@tanstack/react-query";
import {
  MetricsService,
  MetricAnalyticsResponse,
  MetricsListResponse,
} from "@/services/api/metrics";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const metricsQueryKeys = {
  all: ["metrics"] as const,
  analytics: (projectId: string, days: number) =>
    [...metricsQueryKeys.all, "analytics", projectId, { days }] as const,
  list: (projectId: string, days: number, limit: number, offset: number) =>
    [
      ...metricsQueryKeys.all,
      "list",
      projectId,
      { days, limit, offset },
    ] as const,
  detail: (projectId: string, metricName: string, days: number) =>
    [
      ...metricsQueryKeys.all,
      "detail",
      projectId,
      metricName,
      { days },
    ] as const,
};

export function useMetricsAnalytics(
  days: number = 30,
  enabled: boolean = true,
) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<MetricAnalyticsResponse, Error>({
    queryKey: metricsQueryKeys.analytics(projectId, days),
    queryFn: () => MetricsService.getAnalytics(projectId, days),
    enabled,
  });
}

export function useMetricsList(
  days: number = 30,
  limit: number = 20,
  offset: number = 0,
  enabled: boolean = true,
) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<MetricsListResponse, Error>({
    queryKey: metricsQueryKeys.list(projectId, days, limit, offset),
    queryFn: () =>
      MetricsService.getMetricsList(projectId, days, limit, offset),
    enabled,
  });
}

export { metricsQueryKeys };
export default useMetricsAnalytics;
