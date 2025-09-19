import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { WorkflowService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const useWorkflows = () => {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.workflow.list(project.id, branchName),
    queryFn: () => WorkflowService.listWorkflows(project.id, branchName),
  });
};

export default useWorkflows;
