import { useQuery } from "@tanstack/react-query";
import { MetricsService, MetricDetailResponse } from "@/services/api/metrics";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { metricsQueryKeys } from "./useMetricsAnalytics";

export function useMetricDetail(
  metricName: string,
  days: number = 30,
  enabled: boolean = true,
) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<MetricDetailResponse, Error>({
    queryKey: metricsQueryKeys.detail(projectId, metricName, days),
    queryFn: () => MetricsService.getMetricDetail(projectId, metricName, days),
    enabled: enabled && !!metricName,
  });
}

export default useMetricDetail;
