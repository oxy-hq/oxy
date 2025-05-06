import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { service } from "@/services/service";

export const useDeleteAllTasks = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: service.deleteAllTasks,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.task.list() });
    },
  });
};
