import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useRepoCheckout(name: string) {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (branch: string) => RepositoryService.checkoutRepoBranch(projectId, name, branch),
    onSuccess: (_data, branch) => {
      toast.success(`Switched to branch '${branch}'`);
      queryClient.invalidateQueries({ queryKey: queryKeys.repositories.branch(projectId, name) });
      queryClient.invalidateQueries({
        queryKey: queryKeys.repositories.branches(projectId, name)
      });
      queryClient.invalidateQueries({ queryKey: queryKeys.repositories.diff(projectId, name) });
      queryClient.invalidateQueries({
        queryKey: ["repositories", "files", projectId, name]
      });
    },
    onError: (error: Error) => {
      toast.error("Failed to switch branch", { description: error.message });
    }
  });
}
