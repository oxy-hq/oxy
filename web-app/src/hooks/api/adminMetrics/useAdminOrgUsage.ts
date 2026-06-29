import { useQuery } from "@tanstack/react-query";
import { AdminMetricsService } from "@/services/api/adminMetrics";
import queryKeys from "../queryKey";

/**
 * LLM cost + run count + daily trend for a single org over the last `days`.
 *
 * Hits the org-scoped `/admin/metrics/orgs/{id}/llm-usage` endpoint rather than
 * picking the org out of the cross-tenant `by_org` leaderboard — that list is
 * truncated to the top 10 by cost, so any org outside it would wrongly read as
 * zero. This is correct for every tenant and carries `by_day` for the sparkline.
 */
export const useAdminOrgUsage = (orgId: string | undefined, days = 30) =>
  useQuery({
    queryKey: queryKeys.adminMetrics.orgLlmUsage(orgId ?? "", days),
    queryFn: () => AdminMetricsService.orgUsage(orgId as string, days),
    enabled: !!orgId,
    staleTime: 60_000
  });
