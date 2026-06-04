import { useMutation, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { InternalJobsService } from "@/services/api/internalJobs";
import queryKeys from "../queryKey";

export const useReenqueueDead = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (taskId: string) => InternalJobsService.reenqueueDead(taskId),
    onSuccess: () => {
      toast.success("Task re-enqueued");
      qc.invalidateQueries({ queryKey: queryKeys.internalJobs.all });
    },
    onError: (error: unknown) => {
      // Surface the backend's structured `message` (e.g. "Task is in
      // queue_status='completed', only 'dead' rows can be re-enqueued.")
      // rather than Axios's generic "Request failed with status code 409".
      const message = isAxiosError<{ message?: string }>(error)
        ? (error.response?.data?.message ?? error.message)
        : error instanceof Error
          ? error.message
          : "Re-enqueue failed";
      toast.error(message);
    }
  });
};
