import { useMutation, useQueryClient } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";
import { CreateGitNamespaceRequest, GitHubNamespace } from "@/types/github";
import { useNavigate } from "react-router-dom";

/**
 * Hook to create a new Git Namespace
 */
export const useCreateGitNamespace = () => {
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  return useMutation<GitHubNamespace, Error, CreateGitNamespaceRequest>({
    mutationFn: (data) => GitHubApiService.createGitNamespace(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["github", "namespaces"] });
      navigate("/workspaces", {
        state: {
          gitHubInstallSuccess: true,
          message: "GitHub App installation successful!",
        },
      });
    },
  });
};
