import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { TaskCreateRequest, TaskItem } from "@/types/chat";

const useTaskMutation = (onSuccess: (data: TaskItem) => void) => {
  const queryClient = useQueryClient();
  return useMutation<TaskItem, Error, TaskCreateRequest>({
    mutationFn: service.createTask,
    onSuccess: (data: TaskItem) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.task.list() });
      onSuccess(data);
    },
  });
};

export default useTaskMutation;
