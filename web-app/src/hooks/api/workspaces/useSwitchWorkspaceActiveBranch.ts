import { useMutation, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService } from "@/services/api/workspaces";
import queryKeys from "../queryKey";

export const useSwitchWorkspaceActiveBranch = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ workspaceId, branchName }: { workspaceId: string; branchName: string }) =>
      WorkspaceService.switchWorkspaceActiveBranch(workspaceId, branchName),
    onSuccess: (_, variables) => {
      // Invalidate workspace details and branches to refetch
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(variables.workspaceId)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.branches(variables.workspaceId)
      });
    }
  });
};
