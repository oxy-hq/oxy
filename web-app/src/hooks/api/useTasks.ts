import { service } from "@/services/service";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { TaskItem } from "@/types/chat";

const useTasks = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<TaskItem[], Error>({
    queryKey: queryKeys.task.list(),
    queryFn: service.listTasks,
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
  });

export default useTasks;
