import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentWorkspaceBranch from "@/hooks/useCurrentWorkspaceBranch";
import { WorkspaceService } from "@/services/api/workspaces";

const useUpdateGitHubToken = () => {
  const { workspace } = useCurrentWorkspaceBranch();
  return useMutation({
    mutationFn: (token: string) => WorkspaceService.updateGitHubToken(token, workspace.id),
    onSuccess: () => {
      toast.success("GitHub token updated successfully");
    },
    onError: (error) => {
      console.error("Error updating GitHub token:", error);
      toast.error(`Failed to update GitHub token: ${(error as Error).message}`);
    }
  });
};

export default useUpdateGitHubToken;
