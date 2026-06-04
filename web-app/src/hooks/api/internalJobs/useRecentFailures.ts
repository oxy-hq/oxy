import { useQuery } from "@tanstack/react-query";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

const DEFAULT_INTERVAL_MS = 10_000;

export const useRecentFailures = (
  limit: number = 50,
  options: { paused?: boolean; intervalMs?: number } = {}
) => {
  const { paused = false, intervalMs = DEFAULT_INTERVAL_MS } = options;
  return useQuery({
    queryKey: queryKeys.internalJobs.recentFailures(limit),
    queryFn: () => InternalJobsService.recentFailures(limit),
    refetchInterval: paused ? false : intervalMs
  });
};
