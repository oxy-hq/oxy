import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { AutomationService } from "@/services/api";
import { type TaskConfig, TaskType } from "@/stores/useAutomation";
import type { Automation } from "@/types/automation";
import queryKeys from "../queryKey";

interface SaveAutomationRequest {
  name: string;
  description: string;
  tasks: TaskConfig[];
  retrieval?: { include: string[]; exclude: string[] };
}

interface SaveAutomationResponse {
  automation: Automation;
  path: string;
}

const normalizeLookerFilters = (
  filters: Array<{ key: string; value: string }> | undefined
): Record<string, string> | undefined => {
  if (!filters || filters.length === 0) return undefined;

  const mapped = filters.reduce<Record<string, string>>((acc, filter) => {
    const key = filter.key?.trim();
    if (!key) return acc;
    acc[key] = filter.value ?? "";
    return acc;
  }, {});

  return Object.keys(mapped).length > 0 ? mapped : undefined;
};

const normalizeTasksForSave = (tasks: TaskConfig[]): unknown[] =>
  tasks.map((task) => {
    if (task.type !== TaskType.LOOKER_QUERY) {
      return task;
    }

    return {
      ...task,
      filters: normalizeLookerFilters(task.filters),
      sorts: task.sorts
    };
  });

const useSaveAutomationMutation = (onSuccess?: (data: SaveAutomationResponse) => void) => {
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();

  return useMutation<SaveAutomationResponse, Error, SaveAutomationRequest>({
    mutationFn: (request: SaveAutomationRequest) => {
      const normalizedRequest = {
        ...request,
        tasks: normalizeTasksForSave(request.tasks)
      };
      return AutomationService.saveAutomation(project.id, branchName, normalizedRequest);
    },
    onSuccess: (data: SaveAutomationResponse) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.automation.list(project.id, branchName)
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
