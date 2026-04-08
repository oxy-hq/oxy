import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useRemoveRepository() {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (name: string) => RepositoryService.removeRepository(projectId, name),
    onSuccess: () => {
      toast.success("Repository removed");
      queryClient.invalidateQueries({ queryKey: queryKeys.repositories.list(projectId) });
      queryClient.invalidateQueries({ queryKey: ["all", projectId] });
    },
    onError: (error: Error) => {
      toast.error("Failed to remove repository", { description: error.message });
    }
  });
}
