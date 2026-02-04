import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { WorkflowService } from "@/services/api";
import queryKeys from "../queryKey";

const useWorkflowLogs = (relativePath: string) => {
  const { project, branchName } = useCurrentProjectBranch();

  const pathb64 = btoa(relativePath);

  return useQuery({
    queryKey: queryKeys.workflow.getLogs(project.id, branchName, relativePath),
    queryFn: () => WorkflowService.getWorkflowLogs(project.id, branchName, pathb64)
  });
};

export default useWorkflowLogs;
