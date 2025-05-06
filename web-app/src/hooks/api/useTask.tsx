import { service } from "@/services/service";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { TaskItem } from "@/types/chat";

const useTask = (
  taskId: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<TaskItem, Error>({
    queryKey: queryKeys.task.item(taskId),
    queryFn: () => service.getTask(taskId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useTask;
