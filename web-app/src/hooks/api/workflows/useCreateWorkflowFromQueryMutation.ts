import { WorkflowService } from "@/services/api";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { Workflow } from "@/types/workflow";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export interface CreateWorkflowFromQueryRequest {
  query: string;
  prompt: string;
  database: string;
}

export interface WorkflowResponse {
  workflow: Workflow;
}

const useCreateWorkflowFromQueryMutation = (
  onSuccess?: (data: WorkflowResponse) => void,
) => {
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();

  return useMutation<WorkflowResponse, Error, CreateWorkflowFromQueryRequest>({
    mutationFn: (request: CreateWorkflowFromQueryRequest) =>
      WorkflowService.createWorkflowFromQuery(project.id, branchName, request),
    onSuccess: (data: WorkflowResponse) => {
      // Invalidate workflow list queries to refresh the UI
      queryClient.invalidateQueries({
        queryKey: queryKeys.workflow.list(project.id, branchName),
      });
      if (onSuccess) {
        onSuccess(data);
      }
    },
  });
};

export default useCreateWorkflowFromQueryMutation;
