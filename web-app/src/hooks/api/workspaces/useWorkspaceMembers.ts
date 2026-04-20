import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService } from "@/services/api/workspaces";
import type { WorkspaceMember } from "@/types/organization";
import queryKeys from "../queryKey";

export const useWorkspaceMembers = (workspaceId: string, enabled = true) => {
  return useQuery<WorkspaceMember[]>({
    queryKey: queryKeys.workspaces.members(workspaceId),
    queryFn: () => WorkspaceService.getWorkspaceMembers(workspaceId),
    enabled: !!workspaceId && enabled
  });
};

export const useSetWorkspaceRoleOverride = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workspaceId,
      userId,
      role
    }: {
      workspaceId: string;
      userId: string;
      role: string;
    }) => WorkspaceService.setWorkspaceRoleOverride(workspaceId, userId, role),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.members(variables.workspaceId)
      });
    }
  });
};

export const useRemoveWorkspaceRoleOverride = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ workspaceId, userId }: { workspaceId: string; userId: string }) =>
      WorkspaceService.removeWorkspaceRoleOverride(workspaceId, userId),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.members(variables.workspaceId)
      });
    }
  });
};
