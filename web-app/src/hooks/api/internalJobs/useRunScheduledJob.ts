import { useMutation, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

interface RunJobVars {
  name: string;
  triggerPath: string;
}

/**
 * Generic "Run now" for any scheduled job exposing a `trigger_path`
 * (reaper, retention prune, …). Toasts the affected/deleted row count when the
 * endpoint returns one, then refreshes the internal-jobs views.
 */
export const useRunScheduledJob = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (job: RunJobVars) => {
      const data = await InternalJobsService.runScheduled(job.triggerPath);
      return { job, data };
    },
    onSuccess: ({ job, data }) => {
      const n = data.rows_deleted ?? data.rows_affected;
      toast.success(
        n != null ? `${job.name} finished — ${n} row${n === 1 ? "" : "s"}` : `${job.name} finished`
      );
      qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
    },
    onError: (error: unknown, job) => {
      const message = isAxiosError<{ message?: string }>(error)
        ? (error.response?.data?.message ?? error.message)
        : error instanceof Error
          ? error.message
          : `${job.name} failed`;
      toast.error(message);
    }
  });
};
