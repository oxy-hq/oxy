import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useAddRepository() {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: { name: string; path?: string; git_url?: string; branch?: string }) =>
      RepositoryService.addRepository(projectId, request),
    onSuccess: () => {
      toast.success("Repository linked — cloning in background");
      queryClient.invalidateQueries({ queryKey: queryKeys.repositories.list(projectId) });
      queryClient.invalidateQueries({ queryKey: ["all", projectId] });
    },
    onError: (error: Error) => {
      toast.error("Failed to add repository", { description: error.message });
    }
  });
}
