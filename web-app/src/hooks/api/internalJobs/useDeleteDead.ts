import { useMutation, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

export const useDeleteDead = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (taskId: string) => InternalJobsService.deleteDead(taskId),
    onSuccess: () => {
      toast.success("Task deleted");
      qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
    },
    onError: (error: unknown) => {
      const message = isAxiosError<{ message?: string }>(error)
        ? (error.response?.data?.message ?? error.message)
        : error instanceof Error
          ? error.message
          : "Delete failed";
      toast.error(message);
    }
  });
};
