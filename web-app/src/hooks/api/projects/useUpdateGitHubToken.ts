import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ProjectService } from "@/services/api";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";

const useUpdateGitHubToken = () => {
  const { project } = useCurrentProjectBranch();
  return useMutation({
    mutationFn: (token: string) =>
      ProjectService.updateGitHubToken(token, project.id),
    onSuccess: () => {
      toast.success("GitHub token updated successfully");
    },
    onError: (error) => {
      console.error("Error updating GitHub token:", error);
      toast.error("Failed to update GitHub token: " + (error as Error).message);
    },
  });
};

export default useUpdateGitHubToken;
