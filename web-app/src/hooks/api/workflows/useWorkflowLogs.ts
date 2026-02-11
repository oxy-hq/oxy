import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { WorkflowService } from "@/services/api";
import queryKeys from "../queryKey";

const useWorkflowLogs = (relativePath: string) => {
  const { project, branchName } = useCurrentProjectBranch();

  const pathb64 = encodeBase64(relativePath);

  return useQuery({
    queryKey: queryKeys.workflow.getLogs(project.id, branchName, relativePath),
    queryFn: () => WorkflowService.getWorkflowLogs(project.id, branchName, pathb64)
  });
};

export default useWorkflowLogs;
