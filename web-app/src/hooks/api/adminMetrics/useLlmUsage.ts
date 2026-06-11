import { useQuery } from "@tanstack/react-query";
import { AdminMetricsService } from "@/services/api/adminMetrics";
import queryKeys from "../queryKey";

/** Cross-tenant LLM cost + usage overview for the admin dashboard. */
export const useLlmUsage = (days: number) =>
  useQuery({
    queryKey: queryKeys.adminMetrics.llmUsage(days),
    queryFn: () => AdminMetricsService.llmUsage(days),
    staleTime: 60_000
  });
