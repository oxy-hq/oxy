import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { WorkflowService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useWorkflowLogs = (relativePath: string) => {
  const { project, branchName } = useCurrentProjectBranch();

  const pathb64 = btoa(relativePath);

  return useQuery({
    queryKey: queryKeys.workflow.getLogs(project.id, branchName, relativePath),
    queryFn: () =>
      WorkflowService.getWorkflowLogs(project.id, branchName, pathb64),
  });
};

export default useWorkflowLogs;
