import { useQuery } from "@tanstack/react-query";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

const REFETCH_MS = 60_000;

export const useDeadLetter = (limit: number = 50, offset: number = 0) =>
  useQuery({
    queryKey: queryKeys.internalJobs.deadLetter(limit, offset),
    queryFn: () => InternalJobsService.deadLetter(limit, offset),
    refetchInterval: REFETCH_MS
  });
