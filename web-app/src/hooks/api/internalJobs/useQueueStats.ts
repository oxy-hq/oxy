import { useQuery } from "@tanstack/react-query";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

const REFETCH_MS = 30_000;

export const useQueueStats = () =>
  useQuery({
    queryKey: queryKeys.internalJobs.queueStats(),
    queryFn: () => InternalJobsService.queueStats(),
    refetchInterval: REFETCH_MS
  });
