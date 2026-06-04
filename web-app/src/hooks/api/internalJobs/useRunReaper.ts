import { useMutation, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

export const useRunReaper = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => InternalJobsService.runReaper(),
    onSuccess: (data) => {
      toast.success(`Reaper finished — ${data.rows_affected} rows affected`);
      qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
    },
    onError: (error: unknown) => {
      const message = isAxiosError<{ message?: string }>(error)
        ? (error.response?.data?.message ?? error.message)
        : error instanceof Error
          ? error.message
          : "Reaper failed";
      toast.error(message);
    }
  });
};
