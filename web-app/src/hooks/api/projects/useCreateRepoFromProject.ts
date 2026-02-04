import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ProjectService } from "@/services/api";

interface CreateRepoFromProjectRequest {
  projectId: string;
  gitNamespaceId: string;
  repoName: string;
}

export const useCreateRepoFromProject = () => {
  const queryClient = useQueryClient();

  return useMutation<{ success: boolean; message: string }, Error, CreateRepoFromProjectRequest>({
    mutationFn: ({ projectId, gitNamespaceId, repoName }) =>
      ProjectService.createRepoFromProject(projectId, gitNamespaceId, repoName),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["project", variables.projectId]
      });
      queryClient.invalidateQueries({
        queryKey: ["project", variables.projectId, "details"]
      });
      queryClient.invalidateQueries({
        queryKey: ["project", variables.projectId, "status"]
      });
    }
  });
};
