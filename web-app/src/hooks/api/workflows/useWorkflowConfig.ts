import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { WorkflowService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { TaskConfig } from "@/stores/useWorkflow.ts";

const fetchWorkflow = async (
  projectId: string,
  branchName: string,
  relative_path: string,
) => {
  const pathb64 = btoa(relative_path);
  const workflowConfig = await WorkflowService.getWorkflow(
    projectId,
    branchName,
    pathb64,
  );

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
        }),
      ),
  );

  return workflowConfig;
};

const useWorkflowConfig = (relative_path: string) => {
  const { project, branchName } = useCurrentProjectBranch();
  return useQuery({
    queryKey: queryKeys.workflow.get(project.id, branchName, relative_path),
    queryFn: () => fetchWorkflow(project.id, branchName, relative_path),
    enabled: true,
  });
};

export default useWorkflowConfig;
