import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { WorkflowService } from "@/services/api";
import type { TaskConfig } from "@/stores/useWorkflow.ts";
import queryKeys from "../queryKey";

const fetchWorkflow = async (projectId: string, branchName: string, relative_path: string) => {
  const pathb64 = encodeBase64(relative_path);
  const workflowConfig = await WorkflowService.getWorkflow(projectId, branchName, pathb64);

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
        fetchWorkflow(projectId, branchName, task.src).then((subWorkflow) => {
          task.tasks = subWorkflow.tasks;
          return task;
        })
      )
  );

  return workflowConfig;
};

const useWorkflowConfig = (relative_path: string, projectIdOverride?: string) => {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = projectIdOverride ?? project.id;
  return useQuery({
    queryKey: queryKeys.workflow.get(projectId, branchName, relative_path),
    queryFn: () => fetchWorkflow(projectId, branchName, relative_path),
    enabled: true,
    retry: false
  });
};

export default useWorkflowConfig;
