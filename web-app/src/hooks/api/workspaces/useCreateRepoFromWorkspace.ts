import { useMutation, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService } from "@/services/api/workspaces";
import queryKeys from "../queryKey";

interface CreateRepoFromWorkspaceRequest {
  workspaceId: string;
  gitNamespaceId: string;
  repoName: string;
}

export const useCreateRepoFromWorkspace = () => {
  const queryClient = useQueryClient();

  return useMutation<{ success: boolean; message: string }, Error, CreateRepoFromWorkspaceRequest>({
    mutationFn: ({ workspaceId, gitNamespaceId, repoName }) =>
      WorkspaceService.createRepoFromWorkspace(workspaceId, gitNamespaceId, repoName),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(variables.workspaceId)
      });
    }
  });
};
