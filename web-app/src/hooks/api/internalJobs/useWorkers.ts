import { useQuery } from "@tanstack/react-query";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

const REFETCH_MS = 30_000;

export const useWorkers = () =>
  useQuery({
    queryKey: queryKeys.internalJobs.workers(),
    queryFn: () => InternalJobsService.workers(),
    refetchInterval: REFETCH_MS
  });
