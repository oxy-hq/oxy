import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService } from "@/services/api";
import type { CreateWorkspaceRequest } from "@/types/workspace";
import queryKeys from "../queryKey";

export function useWorkspaces() {
  return useQuery({
    queryKey: queryKeys.workspaces.list(),
    queryFn: WorkspaceService.listWorkspaces
  });
}

export function useCreateWorkspace() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateWorkspaceRequest) => WorkspaceService.createWorkspace(data),
    onSuccess: () => {
      // Invalidate and refetch workspaces list
      queryClient.invalidateQueries({ queryKey: queryKeys.workspaces.list() });
    }
  });
}
