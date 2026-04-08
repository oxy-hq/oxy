import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiClient } from "@/services/api/axios";
import type { GitHubAppInstallationRequest } from "@/types/github";

interface UpdateGitHubAppResponse {
  success: boolean;
  message: string;
}

const updateWorkspaceGithubApp = async (
  workspaceId: string,
  installationId: string
): Promise<UpdateGitHubAppResponse> => {
  const response = await apiClient.put(`/github/app/project/${workspaceId}`, {
    installation_id: installationId
  } as GitHubAppInstallationRequest);
  return response.data;
};

export const useUpdateGitHubApp = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workspaceId,
      installationId
    }: {
      workspaceId: string;
      installationId: string;
    }) => updateWorkspaceGithubApp(workspaceId, installationId),
    onSuccess: () => {
      // Invalidate relevant queries
      queryClient.invalidateQueries({ queryKey: ["github", "settings"] });
      queryClient.invalidateQueries({ queryKey: ["project", "current"] });
    }
  });
};

export default useUpdateGitHubApp;
