import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useRepoCommit(name: string) {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (message: string) => RepositoryService.commitRepo(projectId, name, message),
    onSuccess: (data) => {
      toast.success(data.message);
      queryClient.invalidateQueries({ queryKey: queryKeys.repositories.diff(projectId, name) });
      queryClient.invalidateQueries({ queryKey: queryKeys.repositories.branch(projectId, name) });
    },
    onError: (error: Error) => {
      toast.error("Failed to commit", { description: error.message });
    }
  });
}
