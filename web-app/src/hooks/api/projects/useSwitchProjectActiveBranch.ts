import { useMutation, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService as ProjectService } from "@/services/api/workspaces";
import queryKeys from "../queryKey";

export const useSwitchProjectActiveBranch = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ projectId, branchName }: { projectId: string; branchName: string }) =>
      ProjectService.switchWorkspaceActiveBranch(projectId, branchName),
    onSuccess: (_, variables) => {
      // Invalidate workspace details and branches to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(variables.projectId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.branches(variables.projectId)
      });
    }
  });
};
