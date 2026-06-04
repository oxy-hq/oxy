import { useQuery } from "@tanstack/react-query";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

const REFETCH_MS = 30_000;

export const useRecentFailures = (limit: number = 50) =>
  useQuery({
    queryKey: queryKeys.internalJobs.recentFailures(limit),
    queryFn: () => InternalJobsService.recentFailures(limit),
    refetchInterval: REFETCH_MS
  });
