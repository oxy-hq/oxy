import { useMutation, useQueryClient } from "@tanstack/react-query";
import { WorkspaceService } from "@/services/api/workspaces";
import queryKeys from "../queryKey";

interface CreateRepoFromWorkspaceRequest {
  projectId: string;
  gitNamespaceId: string;
  repoName: string;
}

export const useCreateRepoFromProject = () => {
  const queryClient = useQueryClient();

  return useMutation<{ success: boolean; message: string }, Error, CreateRepoFromWorkspaceRequest>({
    mutationFn: ({ projectId, gitNamespaceId, repoName }) =>
      WorkspaceService.createRepoFromWorkspace(projectId, gitNamespaceId, repoName),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(variables.projectId)
      });
    }
  });
};
