import { useQuery } from "@tanstack/react-query";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

const DEFAULT_INTERVAL_MS = 10_000;

export const useDeadLetter = (
  limit: number = 50,
  offset: number = 0,
  options: { paused?: boolean; intervalMs?: number } = {}
) => {
  const { paused = false, intervalMs = DEFAULT_INTERVAL_MS } = options;
  return useQuery({
    queryKey: queryKeys.internalJobs.deadLetter(limit, offset),
    queryFn: () => InternalJobsService.deadLetter(limit, offset),
    refetchInterval: paused ? false : intervalMs
  });
};
