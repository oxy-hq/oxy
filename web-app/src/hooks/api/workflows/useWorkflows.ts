import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { WorkflowService } from "@/services/api";
import queryKeys from "../queryKey";

const useWorkflows = () => {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.workflow.list(project.id, branchName),
    queryFn: () => WorkflowService.listWorkflows(project.id, branchName)
  });
};

export default useWorkflows;
