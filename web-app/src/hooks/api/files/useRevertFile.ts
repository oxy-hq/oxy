import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useRevertFile() {
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (pathb64: string) => FileService.revertFile(project.id, pathb64, branchName),
    onSuccess: (_, pathb64) => {
      // Refresh the file content and git status
      queryClient.removeQueries({
        queryKey: queryKeys.file.get(project.id, pathb64, branchName)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.diffSummary(project.id, branchName)
      });
    },
    onError: () => {
      toast.error("Failed to discard changes");
    }
  });
}
