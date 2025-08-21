import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { apiClient } from "@/services/api/axios";
import { TaskConfig, WorkflowConfig } from "@/stores/useWorkflow.ts";

const fetchWorkflow = async (relative_path: string) => {
  const pathb64 = btoa(relative_path);
  const { data } = await apiClient.get(
    `/workflows/${encodeURIComponent(pathb64)}`,
  );
  const workflowConfig = data.data as WorkflowConfig;

  const deepFlatten = (task: TaskConfig): TaskConfig[] => {
    if (task.type === "loop_sequential") {
      return task.tasks.flatMap(deepFlatten);
    }
    return [task];
  };

  await Promise.all(
    workflowConfig.tasks
      .flatMap(deepFlatten)
      .filter((task) => task.type === "workflow")
      .map((task) =>
        fetchWorkflow(task.src).then((subWorkflow) => {
          task.tasks = subWorkflow.tasks;
          return task;
        }),
      ),
  );

  return workflowConfig;
};

const useWorkflowConfig = (relative_path: string) => {
  return useQuery({
    queryKey: queryKeys.workflow.get(relative_path),
    queryFn: () => fetchWorkflow(relative_path),
    enabled: true,
  });
};

export default useWorkflowConfig;
