import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import type { AddRepositoryFromGitHubRequest } from "@/types/repository";
import queryKeys from "../queryKey";

export default function useAddRepositoryFromGitHub() {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: AddRepositoryFromGitHubRequest) =>
      RepositoryService.addRepositoryFromGitHub(projectId, request),
    onSuccess: () => {
      toast.success("Repository linked — cloning in background");
      queryClient.invalidateQueries({ queryKey: queryKeys.repositories.list(projectId) });
      queryClient.invalidateQueries({ queryKey: ["all", projectId] });
    },
    onError: (error: Error) => {
      toast.error("Failed to link repository", { description: error.message });
    }
  });
}
