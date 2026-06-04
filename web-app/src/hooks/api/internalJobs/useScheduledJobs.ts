import { useQuery } from "@tanstack/react-query";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

export const useScheduledJobs = () =>
  useQuery({
    queryKey: queryKeys.internalJobs.scheduled(),
    queryFn: () => InternalJobsService.scheduled(),
    // Static for now; cache aggressively.
    staleTime: 5 * 60 * 1000
  });
