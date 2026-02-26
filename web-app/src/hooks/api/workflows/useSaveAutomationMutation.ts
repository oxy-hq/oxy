import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { WorkflowService } from "@/services/api";
import type { TaskConfig } from "@/stores/useWorkflow";
import type { Workflow } from "@/types/workflow";
import queryKeys from "../queryKey";

export interface SaveAutomationRequest {
  name: string;
  description: string;
  tasks: TaskConfig[];
  retrieval?: { include: string[]; exclude: string[] };
}

export interface SaveAutomationResponse {
  automation: Workflow;
  path: string;
}

const useSaveAutomationMutation = (onSuccess?: (data: SaveAutomationResponse) => void) => {
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();

  return useMutation<SaveAutomationResponse, Error, SaveAutomationRequest>({
    mutationFn: (request: SaveAutomationRequest) =>
      WorkflowService.saveAutomation(project.id, branchName, request),
    onSuccess: (data: SaveAutomationResponse) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workflow.list(project.id, branchName)
      });
      toast.success("Automation saved successfully");
      if (onSuccess) {
        onSuccess(data);
      }
    },
    onError: (error) => {
      console.error("Failed to save automation:", error);
      toast.error("Failed to save automation");
    }
  });
};

export default useSaveAutomationMutation;
