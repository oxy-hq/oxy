import { service } from "@/services/service";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { Workflow } from "@/types/workflow";

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
  return useMutation<WorkflowResponse, Error, CreateWorkflowFromQueryRequest>({
    mutationFn: (request: CreateWorkflowFromQueryRequest) =>
      service.createWorkflowFromQuery(request),
    onSuccess: (data: WorkflowResponse) => {
      // Invalidate workflow list queries to refresh the UI
      if (queryKeys.workflow?.list) {
        queryClient.invalidateQueries({ queryKey: queryKeys.workflow.list() });
      }
      if (onSuccess) {
        onSuccess(data);
      }
    },
  });
};

export default useCreateWorkflowFromQueryMutation;
