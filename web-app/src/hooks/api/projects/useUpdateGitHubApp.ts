import { useMutation, useQueryClient } from "@tanstack/react-query";
import { apiClient } from "@/services/api/axios";
import { GitHubAppInstallationRequest } from "@/types/github";

interface UpdateGitHubAppResponse {
  success: boolean;
  message: string;
}

const updateProjectGithubApp = async (
  projectId: string,
  installationId: string,
): Promise<UpdateGitHubAppResponse> => {
  const response = await apiClient.put(`/github/app/project/${projectId}`, {
    installation_id: installationId,
  } as GitHubAppInstallationRequest);
  return response.data;
};

export const useUpdateGitHubApp = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      projectId,
      installationId,
    }: {
      projectId: string;
      installationId: string;
    }) => updateProjectGithubApp(projectId, installationId),
    onSuccess: () => {
      // Invalidate relevant queries
      queryClient.invalidateQueries({ queryKey: ["github", "settings"] });
      queryClient.invalidateQueries({ queryKey: ["project", "current"] });
    },
  });
};

export default useUpdateGitHubApp;
